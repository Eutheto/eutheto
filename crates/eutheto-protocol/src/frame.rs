use std::io::{self, Read, Write};

use prost::{Message, bytes::Bytes};

use crate::limits::{FrameClass, ProtocolPolicy, checked_in_policy};
use crate::strict::decode_checked_in;
use crate::wire::{ParentFrame, SolveRequest, WorkerFrame, parent_frame, worker_frame};
use crate::{FrameFault, ProtocolFault};

pub const PARENT_FRAME_MESSAGE: &str = "eutheto.worker.v1.ParentFrame";
pub const WORKER_FRAME_MESSAGE: &str = "eutheto.worker.v1.WorkerFrame";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameSlice<'a> {
    pub payload: &'a [u8],
    pub remainder: &'a [u8],
}

/// Inspects one framed payload without copying it.
///
/// # Errors
///
/// Returns a frame fault when the prefix or payload is truncated, the declared
/// length is zero or exceeds its class cap, or length arithmetic overflows.
pub fn inspect_frame<'a>(
    input: &'a [u8],
    class: FrameClass,
    policy: &ProtocolPolicy,
) -> Result<FrameSlice<'a>, ProtocolFault> {
    if input.len() < 4 {
        return Err(FrameFault::TruncatedPrefix {
            received: input.len(),
        }
        .into());
    }
    let mut prefix = [0u8; 4];
    prefix.copy_from_slice(&input[..4]);
    let length =
        usize::try_from(u32::from_be_bytes(prefix)).map_err(|_| FrameFault::LengthOverflow)?;
    validate_length(length, class, policy)?;
    let end = 4usize
        .checked_add(length)
        .ok_or(FrameFault::LengthOverflow)?;
    if input.len() < end {
        return Err(FrameFault::TruncatedPayload {
            declared: length,
            received: input.len() - 4,
        }
        .into());
    }
    Ok(FrameSlice {
        payload: &input[4..end],
        remainder: &input[end..],
    })
}

/// Reads one bounded payload, returning `None` only for clean EOF before a prefix.
///
/// # Errors
///
/// Returns an I/O or frame fault for read failures, truncated input, invalid
/// lengths, class-cap violations, or bounded allocation failure.
pub fn read_frame<R: Read>(
    reader: &mut R,
    class: FrameClass,
    policy: &ProtocolPolicy,
) -> Result<Option<Bytes>, ProtocolFault> {
    let mut prefix = [0u8; 4];
    let first = read_some(reader, &mut prefix[..1], "reading frame prefix")?;
    if first == 0 {
        return Ok(None);
    }
    let prefix_read = 1 + read_until_eof(reader, &mut prefix[1..], "reading frame prefix")?;
    if prefix_read != 4 {
        return Err(FrameFault::TruncatedPrefix {
            received: prefix_read,
        }
        .into());
    }
    let length =
        usize::try_from(u32::from_be_bytes(prefix)).map_err(|_| FrameFault::LengthOverflow)?;
    validate_length(length, class, policy)?;

    let mut payload = Vec::new();
    payload
        .try_reserve_exact(length)
        .map_err(|_| FrameFault::Allocation)?;
    payload.resize(length, 0);
    let received = read_until_eof(reader, &mut payload, "reading frame payload")?;
    if received != length {
        return Err(FrameFault::TruncatedPayload {
            declared: length,
            received,
        }
        .into());
    }
    Ok(Some(payload.into()))
}

/// Encodes one bounded small protobuf message with the protocol's length prefix.
///
/// Solve-request class frames are rejected; callers must use
/// [`write_solve_request_frame`] so the model payload is not copied into a
/// second maximum-sized output buffer.
///
/// # Errors
///
/// Returns a frame fault for the solve-request class, an empty or oversized
/// payload, length overflow, bounded allocation failure, or protobuf encoding
/// failure.
pub fn encode_frame<M: Message>(
    message: &M,
    class: FrameClass,
    policy: &ProtocolPolicy,
) -> Result<Vec<u8>, ProtocolFault> {
    if class == FrameClass::SolveRequest {
        return Err(FrameFault::StreamingRequired.into());
    }
    encode_frame_allocating(message, class, policy)
}

fn encode_frame_allocating<M: Message>(
    message: &M,
    class: FrameClass,
    policy: &ProtocolPolicy,
) -> Result<Vec<u8>, ProtocolFault> {
    let length = message.encoded_len();
    if length == 0 {
        return Err(FrameFault::EmptyEncoding.into());
    }
    validate_length(length, class, policy)?;
    let encoded_length = u32::try_from(length).map_err(|_| FrameFault::LengthOverflow)?;
    let capacity = length.checked_add(4).ok_or(FrameFault::LengthOverflow)?;
    let mut frame = Vec::new();
    frame
        .try_reserve_exact(capacity)
        .map_err(|_| FrameFault::Allocation)?;
    frame.extend_from_slice(&encoded_length.to_be_bytes());
    message
        .encode(&mut frame)
        .map_err(|error| FrameFault::Encode(error.to_string()))?;
    Ok(frame)
}
/// Streams one solve-request frame without copying its model bytes.
///
/// All lengths and allocations are checked before the first write. Nested
/// allowlist messages reuse one small scratch buffer; the model and fingerprint
/// byte slices are passed directly to the writer.
///
/// # Errors
///
/// Returns a frame, allocation, protobuf encode, or I/O fault.
pub fn write_solve_request_frame<W: Write>(
    writer: &mut W,
    request: &SolveRequest,
    policy: &ProtocolPolicy,
) -> Result<(), ProtocolFault> {
    let solve_length = manual_solve_request_length(request)?;
    if solve_length != request.encoded_len() {
        return Err(FrameFault::SchemaDrift.into());
    }
    let payload_length = 1usize
        .checked_add(varint_length(solve_length))
        .and_then(|length| length.checked_add(solve_length))
        .ok_or(FrameFault::LengthOverflow)?;
    validate_length(payload_length, FrameClass::SolveRequest, policy)?;
    let prefix = u32::try_from(payload_length)
        .map_err(|_| FrameFault::LengthOverflow)?
        .to_be_bytes();

    let scratch_capacity = request
        .parameters
        .as_ref()
        .map_or(0, Message::encoded_len)
        .max(
            request
                .projections
                .iter()
                .map(Message::encoded_len)
                .max()
                .unwrap_or(0),
        )
        .max(
            request
                .resource_limits
                .as_ref()
                .map_or(0, Message::encoded_len),
        );
    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(scratch_capacity)
        .map_err(|_| FrameFault::Allocation)?;

    write_all(writer, &prefix, "writing solve frame prefix")?;
    write_all(writer, &[0x12], "writing parent solve tag")?;
    write_varint(writer, solve_length, "writing parent solve length")?;
    if !request.request_id.is_empty() {
        write_length_delimited(
            writer,
            0x0a,
            request.request_id.as_bytes(),
            "writing solve request ID",
        )?;
    }
    if !request.cp_model_proto.is_empty() {
        write_length_delimited(writer, 0x12, &request.cp_model_proto, "writing solve model")?;
    }
    if let Some(parameters) = &request.parameters {
        write_nested(writer, 0x1a, parameters, &mut scratch)?;
    }
    for projection in &request.projections {
        write_nested(writer, 0x22, projection, &mut scratch)?;
    }
    if let Some(resource_limits) = &request.resource_limits {
        write_nested(writer, 0x2a, resource_limits, &mut scratch)?;
    }
    if !request.model_fingerprint.is_empty() {
        write_length_delimited(
            writer,
            0x32,
            &request.model_fingerprint,
            "writing solve fingerprint",
        )?;
    }
    Ok(())
}

/// Strictly decodes and route-checks a parent frame.
///
/// # Errors
///
/// Returns a wire, decode, policy, descriptor, or wrong-frame-class fault.
pub fn decode_parent_frame(
    payload: Bytes,
    class: FrameClass,
) -> Result<ParentFrame, ProtocolFault> {
    let frame: ParentFrame = decode_checked_in(PARENT_FRAME_MESSAGE, payload)?;
    let correct_class = matches!(
        (&frame.body, class),
        (
            Some(parent_frame::Body::HandshakeRequest(_)),
            FrameClass::Handshake
        ) | (
            Some(parent_frame::Body::SolveRequest(_)),
            FrameClass::SolveRequest
        )
    );
    if !correct_class {
        return Err(FrameFault::WrongClass {
            class: class.policy_name(),
        }
        .into());
    }
    Ok(frame)
}

/// Strictly decodes and route-checks a worker frame.
///
/// # Errors
///
/// Returns a wire, decode, policy, descriptor, or wrong-frame-class fault.
pub fn decode_worker_frame(
    payload: Bytes,
    class: FrameClass,
) -> Result<WorkerFrame, ProtocolFault> {
    let frame: WorkerFrame = decode_checked_in(WORKER_FRAME_MESSAGE, payload)?;
    let correct_class = matches!(
        (&frame.body, class),
        (
            Some(worker_frame::Body::HandshakeResponse(_)),
            FrameClass::Handshake
        ) | (
            Some(
                worker_frame::Body::Started(_)
                    | worker_frame::Body::Progress(_)
                    | worker_frame::Body::Incumbent(_)
                    | worker_frame::Body::Finished(_)
                    | worker_frame::Body::Error(_)
            ),
            FrameClass::WorkerEvent
        )
    );
    if !correct_class {
        return Err(FrameFault::WrongClass {
            class: class.policy_name(),
        }
        .into());
    }
    Ok(frame)
}

/// Inspects one frame using the checked-in protocol policy.
///
/// # Errors
///
/// Returns a policy or frame fault when the checked-in policy is invalid or the
/// framed input violates its selected class.
pub fn inspect_checked_in_frame(
    input: &[u8],
    class: FrameClass,
) -> Result<FrameSlice<'_>, ProtocolFault> {
    inspect_frame(input, class, checked_in_policy()?)
}

fn manual_solve_request_length(request: &SolveRequest) -> Result<usize, ProtocolFault> {
    let mut length = 0usize;
    for field_length in [
        (!request.request_id.is_empty()).then_some(request.request_id.len()),
        (!request.cp_model_proto.is_empty()).then_some(request.cp_model_proto.len()),
        request.parameters.as_ref().map(Message::encoded_len),
    ]
    .into_iter()
    .flatten()
    {
        length = length
            .checked_add(length_delimited_field_length(field_length)?)
            .ok_or(FrameFault::LengthOverflow)?;
    }
    for projection in &request.projections {
        length = length
            .checked_add(length_delimited_field_length(projection.encoded_len())?)
            .ok_or(FrameFault::LengthOverflow)?;
    }
    if let Some(resource_limits) = &request.resource_limits {
        length = length
            .checked_add(length_delimited_field_length(
                resource_limits.encoded_len(),
            )?)
            .ok_or(FrameFault::LengthOverflow)?;
    }
    if !request.model_fingerprint.is_empty() {
        length = length
            .checked_add(length_delimited_field_length(
                request.model_fingerprint.len(),
            )?)
            .ok_or(FrameFault::LengthOverflow)?;
    }
    Ok(length)
}

fn length_delimited_field_length(value_length: usize) -> Result<usize, ProtocolFault> {
    1usize
        .checked_add(varint_length(value_length))
        .and_then(|length| length.checked_add(value_length))
        .ok_or_else(|| FrameFault::LengthOverflow.into())
}

fn write_nested<W: Write, M: Message>(
    writer: &mut W,
    tag: u8,
    message: &M,
    scratch: &mut Vec<u8>,
) -> Result<(), ProtocolFault> {
    scratch.clear();
    message
        .encode(&mut *scratch)
        .map_err(|error| FrameFault::Encode(error.to_string()))?;
    write_length_delimited(writer, tag, scratch, "writing nested solve field")
}

fn write_length_delimited<W: Write>(
    writer: &mut W,
    tag: u8,
    value: &[u8],
    operation: &'static str,
) -> Result<(), ProtocolFault> {
    write_all(writer, &[tag], operation)?;
    write_varint(writer, value.len(), operation)?;
    write_all(writer, value, operation)
}

fn write_varint<W: Write>(
    writer: &mut W,
    mut value: usize,
    operation: &'static str,
) -> Result<(), ProtocolFault> {
    let mut encoded = [0u8; 10];
    let mut length = 0;
    loop {
        let low = u8::try_from(value & 0x7f).map_err(|_| FrameFault::LengthOverflow)?;
        value >>= 7;
        encoded[length] = if value == 0 { low } else { low | 0x80 };
        length += 1;
        if value == 0 {
            break;
        }
    }
    write_all(writer, &encoded[..length], operation)
}

const fn varint_length(mut value: usize) -> usize {
    let mut length = 1;
    while value >= 0x80 {
        value >>= 7;
        length += 1;
    }
    length
}

fn write_all<W: Write>(
    writer: &mut W,
    bytes: &[u8],
    operation: &'static str,
) -> Result<(), ProtocolFault> {
    writer.write_all(bytes).map_err(|error| ProtocolFault::Io {
        operation,
        kind: error.kind(),
    })
}

fn validate_length(
    length: usize,
    class: FrameClass,
    policy: &ProtocolPolicy,
) -> Result<(), ProtocolFault> {
    if length == 0 {
        return Err(FrameFault::ZeroLength.into());
    }
    let cap = policy.frame_cap(class);
    if length > cap {
        return Err(FrameFault::Oversized {
            class: class.policy_name(),
            length,
            cap,
        }
        .into());
    }
    Ok(())
}

fn read_some<R: Read>(
    reader: &mut R,
    output: &mut [u8],
    operation: &'static str,
) -> Result<usize, ProtocolFault> {
    loop {
        match reader.read(output) {
            Ok(read) => return Ok(read),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(ProtocolFault::Io {
                    operation,
                    kind: error.kind(),
                });
            }
        }
    }
}

fn read_until_eof<R: Read>(
    reader: &mut R,
    mut output: &mut [u8],
    operation: &'static str,
) -> Result<usize, ProtocolFault> {
    let expected = output.len();
    while !output.is_empty() {
        let read = read_some(reader, output, operation)?;
        if read == 0 {
            break;
        }
        output = &mut output[read..];
    }
    Ok(expected - output.len())
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use prost::{Message, bytes::Bytes};
    use prost_types::FileDescriptorSet;
    use prost_types::field_descriptor_proto::{Label, Type};

    use super::{encode_frame, encode_frame_allocating, inspect_frame, write_solve_request_frame};
    use crate::limits::{DESCRIPTOR_BYTES, FrameClass};
    use crate::wire::{
        ParentFrame, ProjectionRequest, ResourceLimits, SolveParameters, SolveRequest, parent_frame,
    };
    use crate::{FrameFault, ProtocolFault, checked_in_policy};

    #[test]
    fn exact_frame_boundary_and_remainder_are_borrowed() -> Result<(), ProtocolFault> {
        let policy = checked_in_policy()?;
        let bytes = [0, 0, 0, 1, 0x08, 0xaa];
        let frame = inspect_frame(&bytes, FrameClass::Handshake, policy)?;
        assert_eq!(frame.payload, &[0x08]);
        assert_eq!(frame.remainder, &[0xaa]);
        Ok(())
    }

    #[test]
    fn prefix_and_payload_truncation_are_distinct() -> Result<(), ProtocolFault> {
        let policy = checked_in_policy()?;
        assert!(matches!(
            inspect_frame(&[0, 0, 0], FrameClass::Handshake, policy),
            Err(ProtocolFault::Frame(FrameFault::TruncatedPrefix {
                received: 3
            }))
        ));
        assert!(matches!(
            inspect_frame(&[0, 0, 0, 2, 1], FrameClass::Handshake, policy),
            Err(ProtocolFault::Frame(FrameFault::TruncatedPayload {
                declared: 2,
                received: 1
            }))
        ));
        Ok(())
    }

    #[test]
    fn zero_and_over_cap_lengths_fail_from_prefix_only() -> Result<(), ProtocolFault> {
        let policy = checked_in_policy()?;
        assert!(matches!(
            inspect_frame(&[0, 0, 0, 0], FrameClass::Handshake, policy),
            Err(ProtocolFault::Frame(FrameFault::ZeroLength))
        ));
        let over = u32::try_from(policy.frame_cap(FrameClass::Handshake) + 1)
            .map_err(|_| FrameFault::LengthOverflow)?
            .to_be_bytes();
        assert!(matches!(
            inspect_frame(&over, FrameClass::Handshake, policy),
            Err(ProtocolFault::Frame(FrameFault::Oversized { .. }))
        ));
        Ok(())
    }

    #[test]
    fn every_frame_class_accepts_its_exact_cap_and_rejects_cap_plus_one()
    -> Result<(), ProtocolFault> {
        let policy = checked_in_policy()?;
        for class in [
            FrameClass::Handshake,
            FrameClass::SolveRequest,
            FrameClass::WorkerEvent,
        ] {
            let cap = policy.frame_cap(class);
            let exact = u32::try_from(cap)
                .map_err(|_| FrameFault::LengthOverflow)?
                .to_be_bytes();
            assert!(matches!(
                inspect_frame(&exact, class, policy),
                Err(ProtocolFault::Frame(FrameFault::TruncatedPayload {
                    declared,
                    received: 0
                })) if declared == cap
            ));
            let over = u32::try_from(cap + 1)
                .map_err(|_| FrameFault::LengthOverflow)?
                .to_be_bytes();
            assert!(matches!(
                inspect_frame(&over, class, policy),
                Err(ProtocolFault::Frame(FrameFault::Oversized { .. }))
            ));
        }
        Ok(())
    }

    fn assert_stream_equivalent(request: &SolveRequest) -> Result<(), ProtocolFault> {
        let parent = ParentFrame {
            body: Some(parent_frame::Body::SolveRequest(request.clone())),
        };
        let expected =
            encode_frame_allocating(&parent, FrameClass::SolveRequest, checked_in_policy()?)?;
        let mut actual = Vec::new();
        write_solve_request_frame(&mut actual, request, checked_in_policy()?)?;
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn allocating_encoder_rejects_solve_request_class() -> Result<(), ProtocolFault> {
        let parent = ParentFrame {
            body: Some(parent_frame::Body::SolveRequest(SolveRequest::default())),
        };
        assert!(matches!(
            encode_frame(&parent, FrameClass::SolveRequest, checked_in_policy()?),
            Err(ProtocolFault::Frame(FrameFault::StreamingRequired))
        ));
        Ok(())
    }

    #[test]
    fn streamed_solve_frames_exhaust_current_field_shapes() -> Result<(), ProtocolFault> {
        assert_stream_equivalent(&SolveRequest::default())?;
        assert_stream_equivalent(&SolveRequest {
            parameters: Some(SolveParameters::default()),
            resource_limits: Some(ResourceLimits::default()),
            ..SolveRequest::default()
        })?;
        for value in [false, true] {
            assert_stream_equivalent(&SolveRequest {
                request_id: "request-1".to_owned(),
                cp_model_proto: Bytes::from_static(b"\x08\x01\x12\x00"),
                parameters: Some(SolveParameters {
                    random_seed: Some(-7),
                    stop_after_first_feasible: Some(value),
                    emit_intermediate_solutions: Some(value),
                    log_search_progress: Some(value),
                    deterministic_test_profile: Some(value),
                }),
                projections: vec![
                    ProjectionRequest {
                        projection_id: 0,
                        cp_sat_variable_index: 0,
                    },
                    ProjectionRequest {
                        projection_id: 9,
                        cp_sat_variable_index: -1,
                    },
                ],
                resource_limits: Some(ResourceLimits {
                    wall_time_millis: 10,
                    memory_bytes: Some(20),
                    worker_threads: 3,
                }),
                model_fingerprint: Bytes::from_static(&[3; 32]),
            })?;
        }
        Ok(())
    }

    struct PointerSpy {
        model_pointer: usize,
        model_length: usize,
        observed_direct_model_write: bool,
    }

    impl Write for PointerSpy {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.as_ptr() as usize == self.model_pointer && bytes.len() == self.model_length {
                self.observed_direct_model_write = true;
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn streamed_solve_writes_model_backing_slice_directly() -> Result<(), ProtocolFault> {
        let model = Bytes::from(vec![0x5a; 1024]);
        let request = SolveRequest {
            cp_model_proto: model.clone(),
            ..SolveRequest::default()
        };
        let mut spy = PointerSpy {
            model_pointer: model.as_ptr() as usize,
            model_length: model.len(),
            observed_direct_model_write: false,
        };
        write_solve_request_frame(&mut spy, &request, checked_in_policy()?)?;
        assert!(spy.observed_direct_model_write);
        Ok(())
    }

    #[test]
    fn streamed_solve_descriptor_inventory_is_explicit() -> Result<(), ProtocolFault> {
        let descriptor = FileDescriptorSet::decode(DESCRIPTOR_BYTES)
            .map_err(|error| ProtocolFault::Descriptor(error.to_string()))?;
        let solve = descriptor
            .file
            .iter()
            .flat_map(|file| &file.message_type)
            .find(|message| message.name.as_deref() == Some("SolveRequest"))
            .ok_or_else(|| ProtocolFault::Descriptor("SolveRequest missing".to_owned()))?;
        let inventory = solve
            .field
            .iter()
            .map(|field| (field.number, field.r#type, field.label))
            .collect::<Vec<_>>();
        assert_eq!(
            inventory,
            vec![
                (
                    Some(1),
                    Some(Type::String as i32),
                    Some(Label::Optional as i32)
                ),
                (
                    Some(2),
                    Some(Type::Bytes as i32),
                    Some(Label::Optional as i32)
                ),
                (
                    Some(3),
                    Some(Type::Message as i32),
                    Some(Label::Optional as i32)
                ),
                (
                    Some(4),
                    Some(Type::Message as i32),
                    Some(Label::Repeated as i32)
                ),
                (
                    Some(5),
                    Some(Type::Message as i32),
                    Some(Label::Optional as i32)
                ),
                (
                    Some(6),
                    Some(Type::Bytes as i32),
                    Some(Label::Optional as i32)
                ),
            ]
        );
        Ok(())
    }
}
