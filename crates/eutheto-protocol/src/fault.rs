use std::io;

use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ProtocolFault {
    #[error("invalid checked-in protocol policy: {0}")]
    Policy(String),
    #[error("invalid checked-in protocol descriptor: {0}")]
    Descriptor(String),
    #[error(transparent)]
    Frame(#[from] FrameFault),
    #[error(transparent)]
    Wire(#[from] WireFault),
    #[error("validated {message} payload could not be decoded: {reason}")]
    Decode {
        message: &'static str,
        reason: String,
    },
    #[error(transparent)]
    State(#[from] StateFault),
    #[error("{operation} failed: {kind:?}")]
    Io {
        operation: &'static str,
        kind: io::ErrorKind,
    },
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum FrameFault {
    #[error("frame length prefix is truncated: received {received} of 4 bytes")]
    TruncatedPrefix { received: usize },
    #[error("frame payload length must not be zero")]
    ZeroLength,
    #[error("frame payload length {length} exceeds the {class} cap {cap}")]
    Oversized {
        class: &'static str,
        length: usize,
        cap: usize,
    },
    #[error("decoded frame route does not belong to frame class {class}")]
    WrongClass { class: &'static str },
    #[error("frame payload is truncated: declared {declared}, received {received}")]
    TruncatedPayload { declared: usize, received: usize },
    #[error("frame size arithmetic overflowed")]
    LengthOverflow,
    #[error("memory for a bounded frame could not be reserved")]
    Allocation,
    #[error("encoded protobuf payload is empty")]
    EmptyEncoding,
    #[error("solve requests require the bounded streaming encoder")]
    StreamingRequired,
    #[error("protobuf frame encoding failed: {0}")]
    Encode(String),
    #[error("streaming solve encoder no longer matches the generated schema")]
    SchemaDrift,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("invalid {message} wire payload at byte {offset}: {violation}")]
pub struct WireFault {
    pub message: String,
    pub offset: usize,
    pub violation: WireViolation,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum WireViolation {
    #[error("field tag varint is malformed or truncated")]
    MalformedTag,
    #[error("varint is malformed or truncated")]
    MalformedVarint,
    #[error("varint is not minimally encoded")]
    NonCanonicalVarint,
    #[error("field number is zero or outside the protobuf range")]
    InvalidFieldNumber,
    #[error("wire type {0} is invalid")]
    InvalidWireType(u8),
    #[error("field {0} is reserved and cannot be reused")]
    ReservedField(u32),
    #[error("field {field} has wire type {actual}, expected {expected}")]
    WrongWireType {
        field: String,
        actual: u8,
        expected: String,
    },
    #[error("singular field {0} occurs more than once")]
    DuplicateSingular(String),
    #[error("oneof {oneof} contains both {first} and {second}")]
    ConflictingOneof {
        oneof: String,
        first: String,
        second: String,
    },
    #[error("length-delimited value overflows the input address space")]
    LengthOverflow,
    #[error("length-delimited value is truncated")]
    TruncatedLength,
    #[error("fixed-width value is truncated")]
    TruncatedFixed,
    #[error("message nesting depth exceeds {0}")]
    NestingDepth(usize),
    #[error("repeated field {field} has more than {cap} values")]
    RepeatedCount { field: String, cap: usize },
    #[error("field {field} has {length} bytes, exceeding its cap {cap}")]
    FieldBytes {
        field: String,
        length: usize,
        cap: usize,
    },
    #[error("boolean field {field} contains value {value}, not zero or one")]
    InvalidBool { field: String, value: u64 },
    #[error("integer field {field} contains out-of-range wire value {value}")]
    IntegerRange { field: String, value: u64 },
    #[error("string field {0} is not UTF-8")]
    InvalidUtf8(String),
    #[error("groups are not supported")]
    Group,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum StateFault {
    #[error("unexpected parent frame while in {state}: {message}")]
    UnexpectedParent {
        state: &'static str,
        message: &'static str,
    },
    #[error("unexpected worker frame while in {state}: {message}")]
    UnexpectedWorker {
        state: &'static str,
        message: &'static str,
    },
    #[error("worker frame is missing its required body")]
    MissingWorkerBody,
    #[error("parent frame is missing its required body")]
    MissingParentBody,
    #[error("handshake contains the unspecified capability")]
    InvalidCapability,
    #[error("handshake repeats capability {0}")]
    DuplicateCapability(i32),
    #[error("capability {0} is disabled by a security gate")]
    CapabilityDisabled(i32),
    #[error("handshake has an invalid {0}")]
    InvalidHandshake(&'static str),
    #[error("worker model fingerprint does not match the solve request")]
    ModelFingerprint,
    #[error("parent solve request has an invalid {0}")]
    InvalidSolveRequest(&'static str),
    #[error(
        "protocol version mismatch: expected {expected_major}.{expected_minor}, received {actual_major}.{actual_minor}"
    )]
    ProtocolVersion {
        expected_major: u32,
        expected_minor: u32,
        actual_major: u32,
        actual_minor: u32,
    },
    #[error("{field} is not a semantic version")]
    InvalidVersion { field: &'static str },
    #[error("{0} does not match the expected version")]
    VersionMismatch(&'static str),
    #[error("worker backend identity does not match the expected backend")]
    BackendIdentity,
    #[error("worker manifest SHA-256 does not match the expected manifest")]
    ManifestSha256,
    #[error("worker is missing required capability {0}")]
    MissingCapability(i32),
    #[error("worker field {field} requires unnegotiated capability {capability}")]
    UnnegotiatedCapability {
        field: &'static str,
        capability: i32,
    },
    #[error("worker event request ID does not match the active request")]
    RequestId,
    #[error("worker frame has an invalid {0}")]
    InvalidWorkerField(&'static str),
    #[error("projected candidate repeats projection ID {0}")]
    DuplicateProjection(u64),
    #[error("projected candidate contains unrequested projection ID {0}")]
    UnrequestedProjection(u64),
    #[error("projected candidate is missing requested projection ID {0}")]
    MissingProjection(u64),
    #[error("worker output ended before a terminal frame")]
    MissingTerminal,
    #[error("worker exited with code {0} after a terminal frame")]
    ContradictoryExit(i32),
    #[error("worker exit was observed before clean EOF")]
    ExitBeforeEof,
    #[error("worker exit was observed more than once")]
    DuplicateExit,
    #[error("stderr retention limit is smaller than its truncation marker")]
    StderrLimit,
}
