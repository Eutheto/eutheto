use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use prost::{Message, bytes::Bytes};
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{DescriptorProto, EnumDescriptorProto, FileDescriptorSet};

use crate::limits::{DESCRIPTOR_BYTES, ProtocolPolicy, checked_in_policy};
use crate::{ProtocolFault, WireFault, WireViolation};

#[derive(Debug)]
pub struct WireSchema {
    messages: BTreeMap<String, MessageSchema>,
    enums: BTreeMap<String, BTreeSet<i32>>,
}

#[derive(Debug)]
struct MessageSchema {
    fields: BTreeMap<u32, FieldSchema>,
    reserved_ranges: Vec<(u32, u32)>,
}

#[derive(Debug)]
struct FieldSchema {
    name: String,
    full_name: String,
    label: Label,
    kind: Type,
    type_name: Option<String>,
    oneof: Option<String>,
}

impl WireSchema {
    /// Builds a strict wire schema from a descriptor and validates its policy keys.
    ///
    /// # Errors
    ///
    /// Returns a descriptor or policy fault for malformed descriptors, duplicate
    /// declarations, unresolved types, or invalid field-limit references.
    pub fn decode(descriptor_bytes: &[u8], policy: &ProtocolPolicy) -> Result<Self, ProtocolFault> {
        let descriptor = FileDescriptorSet::decode(descriptor_bytes)
            .map_err(|error| ProtocolFault::Descriptor(error.to_string()))?;
        let mut schema = Self {
            messages: BTreeMap::new(),
            enums: BTreeMap::new(),
        };
        for file in descriptor.file {
            let package = file.package.unwrap_or_default();
            for enumeration in file.enum_type {
                schema.add_enum(&package, &enumeration)?;
            }
            for message in file.message_type {
                schema.add_message(&package, &message)?;
            }
        }
        if schema.messages.is_empty() {
            return Err(ProtocolFault::Descriptor(
                "descriptor set contains no messages".to_owned(),
            ));
        }
        schema.validate_policy(policy)?;
        Ok(schema)
    }

    /// Validates one protobuf payload before generated-code decoding.
    ///
    /// # Errors
    ///
    /// Returns a descriptor or wire fault for an unknown message or any bounded,
    /// structural, encoding, field, enum, count, depth, or UTF-8 violation.
    pub fn validate(
        &self,
        message_name: &str,
        bytes: &[u8],
        policy: &ProtocolPolicy,
    ) -> Result<(), ProtocolFault> {
        self.validate_nested(message_name, bytes, policy, 1, 0)
    }

    #[allow(clippy::too_many_lines)]
    fn add_message(
        &mut self,
        parent: &str,
        descriptor: &DescriptorProto,
    ) -> Result<(), ProtocolFault> {
        let short_name = required_name(descriptor.name.as_deref(), "message")?;
        let full_name = qualify(parent, short_name);
        let oneofs: Vec<&str> = descriptor
            .oneof_decl
            .iter()
            .map(|oneof| required_name(oneof.name.as_deref(), "oneof"))
            .collect::<Result<_, _>>()?;
        let mut reserved_ranges = Vec::with_capacity(descriptor.reserved_range.len());
        for range in &descriptor.reserved_range {
            let start = u32::try_from(range.start.ok_or_else(|| {
                ProtocolFault::Descriptor(format!(
                    "message {full_name} has a reserved range without a start"
                ))
            })?)
            .map_err(|_| {
                ProtocolFault::Descriptor(format!(
                    "message {full_name} has an invalid reserved range start"
                ))
            })?;
            let end = u32::try_from(range.end.ok_or_else(|| {
                ProtocolFault::Descriptor(format!(
                    "message {full_name} has a reserved range without an end"
                ))
            })?)
            .map_err(|_| {
                ProtocolFault::Descriptor(format!(
                    "message {full_name} has an invalid reserved range end"
                ))
            })?;
            if start == 0 || start >= end || end > 536_870_912 || (start < 20_000 && end > 19_000) {
                return Err(ProtocolFault::Descriptor(format!(
                    "message {full_name} has an invalid reserved range"
                )));
            }
            reserved_ranges.push((start, end));
        }
        reserved_ranges.sort_unstable();
        if reserved_ranges
            .windows(2)
            .any(|ranges| ranges[0].1 > ranges[1].0)
        {
            return Err(ProtocolFault::Descriptor(format!(
                "message {full_name} has overlapping reserved ranges"
            )));
        }
        let mut fields = BTreeMap::new();
        for field in &descriptor.field {
            let name = required_name(field.name.as_deref(), "field")?.to_owned();
            let number_i32 = field.number.ok_or_else(|| {
                ProtocolFault::Descriptor(format!("field {full_name}.{name} has no number"))
            })?;
            let number = u32::try_from(number_i32).map_err(|_| {
                ProtocolFault::Descriptor(format!("field {full_name}.{name} has invalid number"))
            })?;
            if number == 0
                || number > 536_870_911
                || (19_000..=19_999).contains(&number)
                || reserved_ranges
                    .iter()
                    .any(|(start, end)| (*start..*end).contains(&number))
                || fields.contains_key(&number)
            {
                return Err(ProtocolFault::Descriptor(format!(
                    "field {full_name}.{name} has an invalid or duplicate number"
                )));
            }
            let label = Label::try_from(field.label.ok_or_else(|| {
                ProtocolFault::Descriptor(format!("field {full_name}.{name} has no label"))
            })?)
            .map_err(|_| {
                ProtocolFault::Descriptor(format!("field {full_name}.{name} has invalid label"))
            })?;
            let kind = Type::try_from(field.r#type.ok_or_else(|| {
                ProtocolFault::Descriptor(format!("field {full_name}.{name} has no type"))
            })?)
            .map_err(|_| {
                ProtocolFault::Descriptor(format!("field {full_name}.{name} has invalid type"))
            })?;
            let oneof = match field.oneof_index {
                Some(index) => {
                    let index = usize::try_from(index).map_err(|_| {
                        ProtocolFault::Descriptor(format!(
                            "field {full_name}.{name} has invalid oneof index"
                        ))
                    })?;
                    Some(
                        oneofs
                            .get(index)
                            .ok_or_else(|| {
                                ProtocolFault::Descriptor(format!(
                                    "field {full_name}.{name} has unknown oneof index"
                                ))
                            })?
                            .to_string(),
                    )
                }
                None => None,
            };
            fields.insert(
                number,
                FieldSchema {
                    full_name: format!("{full_name}.{name}"),
                    name,
                    label,
                    kind,
                    type_name: field
                        .type_name
                        .as_deref()
                        .map(|value| value.trim_start_matches('.').to_owned()),
                    oneof,
                },
            );
        }
        if self
            .messages
            .insert(
                full_name.clone(),
                MessageSchema {
                    fields,
                    reserved_ranges,
                },
            )
            .is_some()
        {
            return Err(ProtocolFault::Descriptor(format!(
                "duplicate message {full_name}"
            )));
        }
        for enumeration in &descriptor.enum_type {
            self.add_enum(&full_name, enumeration)?;
        }
        for nested in &descriptor.nested_type {
            self.add_message(&full_name, nested)?;
        }
        Ok(())
    }

    fn add_enum(
        &mut self,
        parent: &str,
        descriptor: &EnumDescriptorProto,
    ) -> Result<(), ProtocolFault> {
        let name = required_name(descriptor.name.as_deref(), "enum")?;
        let full_name = qualify(parent, name);
        let mut values = BTreeSet::new();
        for value in &descriptor.value {
            values.insert(value.number.ok_or_else(|| {
                ProtocolFault::Descriptor(format!("enum {full_name} has an unnumbered value"))
            })?);
        }
        if values.is_empty() || self.enums.insert(full_name.clone(), values).is_some() {
            return Err(ProtocolFault::Descriptor(format!(
                "enum {full_name} is empty or duplicated"
            )));
        }
        Ok(())
    }

    fn validate_policy(&self, policy: &ProtocolPolicy) -> Result<(), ProtocolFault> {
        for (name, limit) in policy.field_limits() {
            let (message_name, field_name) = name.rsplit_once('.').ok_or_else(|| {
                ProtocolFault::Policy(format!("field limit key {name:?} is not fully qualified"))
            })?;
            let message = self.messages.get(message_name).ok_or_else(|| {
                ProtocolFault::Policy(format!("field limit names unknown message {message_name}"))
            })?;
            let field = message
                .fields
                .values()
                .find(|field| field.name == field_name)
                .ok_or_else(|| {
                    ProtocolFault::Policy(format!("field limit names unknown field {name}"))
                })?;
            if limit.max_count().is_some() && field.label != Label::Repeated {
                return Err(ProtocolFault::Policy(format!(
                    "non-repeated field {name} has max_count"
                )));
            }
            if limit.max_bytes().is_some() && !matches!(field.kind, Type::String | Type::Bytes) {
                return Err(ProtocolFault::Policy(format!(
                    "non-byte field {name} has max_bytes"
                )));
            }
        }
        for (message_name, message) in &self.messages {
            for field in message.fields.values() {
                if matches!(field.kind, Type::Message | Type::Enum) {
                    let target = field.type_name.as_deref().ok_or_else(|| {
                        ProtocolFault::Descriptor(format!(
                            "field {message_name}.{} has no type name",
                            field.name
                        ))
                    })?;
                    let found = if field.kind == Type::Message {
                        self.messages.contains_key(target)
                    } else {
                        self.enums.contains_key(target)
                    };
                    if !found {
                        return Err(ProtocolFault::Descriptor(format!(
                            "field {message_name}.{} refers to unknown type {target}",
                            field.name
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn validate_nested(
        &self,
        message_name: &str,
        bytes: &[u8],
        policy: &ProtocolPolicy,
        depth: usize,
        base_offset: usize,
    ) -> Result<(), ProtocolFault> {
        if depth > policy.max_nesting_depth() {
            return Err(wire_fault(
                message_name,
                base_offset,
                WireViolation::NestingDepth(policy.max_nesting_depth()),
            ));
        }
        let message = self.messages.get(message_name).ok_or_else(|| {
            ProtocolFault::Descriptor(format!("message {message_name} is not in the descriptor"))
        })?;
        let mut position = 0;
        let mut singular = BTreeSet::new();
        let mut oneofs: BTreeMap<&str, &str> = BTreeMap::new();
        let mut repeated_counts: BTreeMap<u32, usize> = BTreeMap::new();
        let mut unknown_fields = 0usize;
        while position < bytes.len() {
            let tag_offset = position;
            let (tag, after_tag) = parse_varint(bytes, position).map_err(|error| {
                wire_fault(
                    message_name,
                    base_offset + tag_offset,
                    match error {
                        VarintError::NonCanonical => WireViolation::NonCanonicalVarint,
                        VarintError::Malformed => WireViolation::MalformedTag,
                    },
                )
            })?;
            position = after_tag;
            let wire_type = u8::try_from(tag & 7).map_err(|_| {
                wire_fault(
                    message_name,
                    base_offset + tag_offset,
                    WireViolation::MalformedTag,
                )
            })?;
            let number_value = tag >> 3;
            if number_value == 0
                || number_value > 536_870_911
                || (19_000..=19_999).contains(&number_value)
            {
                return Err(wire_fault(
                    message_name,
                    base_offset + tag_offset,
                    WireViolation::InvalidFieldNumber,
                ));
            }
            let number = u32::try_from(number_value).map_err(|_| {
                wire_fault(
                    message_name,
                    base_offset + tag_offset,
                    WireViolation::InvalidFieldNumber,
                )
            })?;
            let Some(field) = message.fields.get(&number) else {
                unknown_fields = unknown_fields.checked_add(1).ok_or_else(|| {
                    wire_fault(
                        message_name,
                        base_offset + tag_offset,
                        WireViolation::RepeatedCount {
                            field: "unknown fields".to_owned(),
                            cap: policy.max_repeated_field_items(),
                        },
                    )
                })?;
                if unknown_fields > policy.max_repeated_field_items() {
                    return Err(wire_fault(
                        message_name,
                        base_offset + tag_offset,
                        WireViolation::RepeatedCount {
                            field: "unknown fields".to_owned(),
                            cap: policy.max_repeated_field_items(),
                        },
                    ));
                }
                if (19_000..=19_999).contains(&number)
                    || message
                        .reserved_ranges
                        .iter()
                        .any(|(start, end)| (*start..*end).contains(&number))
                {
                    return Err(wire_fault(
                        message_name,
                        base_offset + tag_offset,
                        WireViolation::ReservedField(number),
                    ));
                }
                position = Self::skip_unknown_value(
                    message_name,
                    bytes,
                    position,
                    wire_type,
                    base_offset,
                )?;
                continue;
            };
            if field.label != Label::Repeated && !singular.insert(number) {
                return Err(wire_fault(
                    message_name,
                    base_offset + tag_offset,
                    WireViolation::DuplicateSingular(field.name.clone()),
                ));
            }
            if let Some(oneof) = field.oneof.as_deref()
                && let Some(first) = oneofs.insert(oneof, &field.name)
                && first != field.name
            {
                return Err(wire_fault(
                    message_name,
                    base_offset + tag_offset,
                    WireViolation::ConflictingOneof {
                        oneof: oneof.to_owned(),
                        first: first.to_owned(),
                        second: field.name.clone(),
                    },
                ));
            }

            let repeated_cap = policy
                .field_limit(&field.full_name)
                .max_count()
                .unwrap_or(policy.max_repeated_field_items())
                .min(policy.max_repeated_field_items());
            let observed_count = repeated_counts.get(&number).copied().unwrap_or(0);
            let packed =
                field.label == Label::Repeated && is_packable(field.kind) && wire_type == 2;
            let added = if packed {
                let (payload, next, payload_offset) =
                    take_length(bytes, position).map_err(|violation| {
                        wire_fault(message_name, base_offset + position, violation)
                    })?;
                position = next;
                Self::validate_packed(
                    message_name,
                    field,
                    payload,
                    base_offset + payload_offset,
                    repeated_cap.saturating_sub(observed_count),
                    repeated_cap,
                )?
            } else {
                let expected = natural_wire_type(field.kind);
                if wire_type != expected {
                    return Err(wire_fault(
                        message_name,
                        base_offset + tag_offset,
                        WireViolation::WrongWireType {
                            field: field.name.clone(),
                            actual: wire_type,
                            expected: if field.label == Label::Repeated && is_packable(field.kind) {
                                format!("{expected} or 2")
                            } else {
                                expected.to_string()
                            },
                        },
                    ));
                }
                position = self.validate_value(
                    message_name,
                    field,
                    bytes,
                    position,
                    policy,
                    depth,
                    base_offset,
                )?;
                1
            };
            if field.label == Label::Repeated {
                let count = repeated_counts.entry(number).or_default();
                *count = count.checked_add(added).ok_or_else(|| {
                    wire_fault(
                        message_name,
                        base_offset + tag_offset,
                        WireViolation::RepeatedCount {
                            field: field.name.clone(),
                            cap: repeated_cap,
                        },
                    )
                })?;
                if *count > repeated_cap {
                    return Err(wire_fault(
                        message_name,
                        base_offset + tag_offset,
                        WireViolation::RepeatedCount {
                            field: field.name.clone(),
                            cap: repeated_cap,
                        },
                    ));
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_value(
        &self,
        message_name: &str,
        field: &FieldSchema,
        bytes: &[u8],
        position: usize,
        policy: &ProtocolPolicy,
        depth: usize,
        base_offset: usize,
    ) -> Result<usize, ProtocolFault> {
        match field.kind {
            Type::Double | Type::Fixed64 | Type::Sfixed64 => fixed_end(bytes, position, 8)
                .map_err(|violation| wire_fault(message_name, base_offset + position, violation)),
            Type::Float | Type::Fixed32 | Type::Sfixed32 => fixed_end(bytes, position, 4)
                .map_err(|violation| wire_fault(message_name, base_offset + position, violation)),
            Type::String | Type::Bytes | Type::Message => {
                let (payload, next, payload_offset) =
                    take_length(bytes, position).map_err(|violation| {
                        wire_fault(message_name, base_offset + position, violation)
                    })?;
                let limit = policy.field_limit(&field.full_name);
                let cap = limit.max_bytes().unwrap_or_else(|| {
                    if field.kind == Type::String {
                        policy.max_string_bytes()
                    } else {
                        bytes.len()
                    }
                });
                if payload.len() > cap {
                    return Err(wire_fault(
                        message_name,
                        base_offset + payload_offset,
                        WireViolation::FieldBytes {
                            field: field.name.clone(),
                            length: payload.len(),
                            cap,
                        },
                    ));
                }
                if field.kind == Type::String && std::str::from_utf8(payload).is_err() {
                    return Err(wire_fault(
                        message_name,
                        base_offset + payload_offset,
                        WireViolation::InvalidUtf8(field.name.clone()),
                    ));
                }
                if field.kind == Type::Message {
                    let nested_name = field.type_name.as_deref().ok_or_else(|| {
                        ProtocolFault::Descriptor(format!(
                            "field {} has no message type",
                            field.full_name
                        ))
                    })?;
                    self.validate_nested(
                        nested_name,
                        payload,
                        policy,
                        depth + 1,
                        base_offset + payload_offset,
                    )?;
                }
                Ok(next)
            }
            Type::Enum
            | Type::Bool
            | Type::Int32
            | Type::Int64
            | Type::Uint32
            | Type::Uint64
            | Type::Sint32
            | Type::Sint64 => {
                let (value, next) = parse_varint(bytes, position).map_err(|error| {
                    wire_fault(
                        message_name,
                        base_offset + position,
                        varint_violation(error),
                    )
                })?;
                Self::validate_varint_value(message_name, field, value, base_offset + position)?;
                Ok(next)
            }
            Type::Group => Err(wire_fault(
                message_name,
                base_offset + position,
                WireViolation::Group,
            )),
        }
    }

    fn validate_packed(
        message_name: &str,
        field: &FieldSchema,
        payload: &[u8],
        base_offset: usize,
        remaining: usize,
        cap: usize,
    ) -> Result<usize, ProtocolFault> {
        let width = match field.kind {
            Type::Double | Type::Fixed64 | Type::Sfixed64 => Some(8),
            Type::Float | Type::Fixed32 | Type::Sfixed32 => Some(4),
            _ => None,
        };
        if let Some(width) = width {
            if !payload.len().is_multiple_of(width) {
                return Err(wire_fault(
                    message_name,
                    base_offset + payload.len(),
                    WireViolation::TruncatedFixed,
                ));
            }
            let count = payload.len() / width;
            if count > remaining {
                return Err(wire_fault(
                    message_name,
                    base_offset + remaining.saturating_mul(width),
                    WireViolation::RepeatedCount {
                        field: field.name.clone(),
                        cap,
                    },
                ));
            }
            return Ok(count);
        }
        let mut count = 0usize;
        let mut position = 0usize;
        while position < payload.len() {
            if count == remaining {
                return Err(wire_fault(
                    message_name,
                    base_offset + position,
                    WireViolation::RepeatedCount {
                        field: field.name.clone(),
                        cap,
                    },
                ));
            }
            let offset = position;
            let (value, next) = parse_varint(payload, position).map_err(|error| {
                wire_fault(message_name, base_offset + offset, varint_violation(error))
            })?;
            Self::validate_varint_value(message_name, field, value, base_offset + offset)?;
            count += 1;
            position = next;
        }
        Ok(count)
    }

    fn validate_varint_value(
        message_name: &str,
        field: &FieldSchema,
        value: u64,
        offset: usize,
    ) -> Result<(), ProtocolFault> {
        let range_valid = match field.kind {
            Type::Int32 => decode_i32_wire(value).is_some(),
            Type::Uint32 | Type::Sint32 => u32::try_from(value).is_ok(),
            _ => true,
        };
        if !range_valid {
            return Err(wire_fault(
                message_name,
                offset,
                WireViolation::IntegerRange {
                    field: field.name.clone(),
                    value,
                },
            ));
        }
        if field.kind == Type::Bool && value > 1 {
            return Err(wire_fault(
                message_name,
                offset,
                WireViolation::InvalidBool {
                    field: field.name.clone(),
                    value,
                },
            ));
        }
        if field.kind == Type::Enum && decode_i32_wire(value).is_none() {
            return Err(wire_fault(
                message_name,
                offset,
                WireViolation::IntegerRange {
                    field: field.name.clone(),
                    value,
                },
            ));
        }
        Ok(())
    }
    fn skip_unknown_value(
        message_name: &str,
        bytes: &[u8],
        position: usize,
        wire_type: u8,
        base_offset: usize,
    ) -> Result<usize, ProtocolFault> {
        match wire_type {
            0 => parse_varint(bytes, position)
                .map(|(_, next)| next)
                .map_err(|error| {
                    wire_fault(
                        message_name,
                        base_offset + position,
                        varint_violation(error),
                    )
                }),
            1 => fixed_end(bytes, position, 8)
                .map_err(|violation| wire_fault(message_name, base_offset + position, violation)),
            2 => take_length(bytes, position)
                .map(|(_, next, _)| next)
                .map_err(|violation| wire_fault(message_name, base_offset + position, violation)),
            5 => fixed_end(bytes, position, 4)
                .map_err(|violation| wire_fault(message_name, base_offset + position, violation)),
            3 | 4 => Err(wire_fault(
                message_name,
                base_offset + position,
                WireViolation::Group,
            )),
            _ => Err(wire_fault(
                message_name,
                base_offset + position,
                WireViolation::InvalidWireType(wire_type),
            )),
        }
    }
}

/// Returns the strict schema embedded in this crate.
///
/// # Errors
///
/// Returns the cached policy or descriptor fault when checked-in inputs are invalid.
pub fn checked_in_schema() -> Result<&'static WireSchema, ProtocolFault> {
    static SCHEMA: LazyLock<Result<WireSchema, ProtocolFault>> = LazyLock::new(|| {
        let policy = checked_in_policy()?;
        WireSchema::decode(DESCRIPTOR_BYTES, policy)
    });
    match &*SCHEMA {
        Ok(schema) => Ok(schema),
        Err(error) => Err(error.clone()),
    }
}

/// Strictly validates bytes against the checked-in schema and policy.
///
/// # Errors
///
/// Returns a policy, descriptor, or wire fault for invalid checked-in inputs or
/// payload bytes.
pub fn validate_checked_in(message_name: &str, bytes: &[u8]) -> Result<(), ProtocolFault> {
    let policy = checked_in_policy()?;
    checked_in_schema()?.validate(message_name, bytes, policy)
}

/// Strictly validates and decodes one checked-in protobuf message.
///
/// # Errors
///
/// Returns a policy, descriptor, wire, or protobuf decode fault.
pub(crate) fn decode_checked_in<M>(
    message_name: &'static str,
    bytes: Bytes,
) -> Result<M, ProtocolFault>
where
    M: Message + Default,
{
    validate_checked_in(message_name, &bytes)?;
    M::decode(bytes).map_err(|error| ProtocolFault::Decode {
        message: message_name,
        reason: error.to_string(),
    })
}

fn required_name<'a>(value: Option<&'a str>, kind: &str) -> Result<&'a str, ProtocolFault> {
    value
        .filter(|name| !name.is_empty())
        .ok_or_else(|| ProtocolFault::Descriptor(format!("{kind} is missing its name")))
}

fn qualify(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}.{name}")
    }
}

fn natural_wire_type(kind: Type) -> u8 {
    match kind {
        Type::Double | Type::Fixed64 | Type::Sfixed64 => 1,
        Type::String | Type::Bytes | Type::Message => 2,
        Type::Float | Type::Fixed32 | Type::Sfixed32 => 5,
        Type::Group => 3,
        Type::Enum
        | Type::Bool
        | Type::Int32
        | Type::Int64
        | Type::Uint32
        | Type::Uint64
        | Type::Sint32
        | Type::Sint64 => 0,
    }
}

fn is_packable(kind: Type) -> bool {
    !matches!(
        kind,
        Type::String | Type::Bytes | Type::Message | Type::Group
    )
}

fn fixed_end(bytes: &[u8], position: usize, width: usize) -> Result<usize, WireViolation> {
    let end = position
        .checked_add(width)
        .ok_or(WireViolation::LengthOverflow)?;
    if end > bytes.len() {
        return Err(WireViolation::TruncatedFixed);
    }
    Ok(end)
}

fn take_length(bytes: &[u8], position: usize) -> Result<(&[u8], usize, usize), WireViolation> {
    let (length, payload_offset) = parse_varint(bytes, position).map_err(varint_violation)?;
    let length = usize::try_from(length).map_err(|_| WireViolation::LengthOverflow)?;
    let end = payload_offset
        .checked_add(length)
        .ok_or(WireViolation::LengthOverflow)?;
    if end > bytes.len() {
        return Err(WireViolation::TruncatedLength);
    }
    Ok((&bytes[payload_offset..end], end, payload_offset))
}

fn decode_i32_wire(value: u64) -> Option<i32> {
    let bytes = value.to_le_bytes();
    let numeric = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let canonical = if numeric.is_negative() {
        u64::from_ne_bytes(i64::from(numeric).to_ne_bytes())
    } else {
        u64::try_from(numeric).ok()?
    };
    (value == canonical).then_some(numeric)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VarintError {
    Malformed,
    NonCanonical,
}

fn parse_varint(bytes: &[u8], start: usize) -> Result<(u64, usize), VarintError> {
    let mut value = 0u64;
    for index in 0..10usize {
        let position = start.checked_add(index).ok_or(VarintError::Malformed)?;
        let byte = *bytes.get(position).ok_or(VarintError::Malformed)?;
        if index == 9 && byte > 1 {
            return Err(VarintError::Malformed);
        }
        value |= u64::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            let encoded = index + 1;
            if encoded != canonical_varint_len(value) {
                return Err(VarintError::NonCanonical);
            }
            return Ok((value, position + 1));
        }
    }
    Err(VarintError::Malformed)
}

fn canonical_varint_len(mut value: u64) -> usize {
    let mut length = 1;
    while value >= 0x80 {
        value >>= 7;
        length += 1;
    }
    length
}

fn varint_violation(error: VarintError) -> WireViolation {
    match error {
        VarintError::Malformed => WireViolation::MalformedVarint,
        VarintError::NonCanonical => WireViolation::NonCanonicalVarint,
    }
}

fn wire_fault(message: &str, offset: usize, violation: WireViolation) -> ProtocolFault {
    WireFault {
        message: message.to_owned(),
        offset,
        violation,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::{canonical_varint_len, parse_varint, validate_checked_in};
    use crate::{ProtocolFault, WireViolation};

    fn append_varint(bytes: &mut Vec<u8>, mut value: usize) {
        loop {
            let low = u8::try_from(value & 0x7f).unwrap_or(0);
            value >>= 7;
            bytes.push(if value == 0 { low } else { low | 0x80 });
            if value == 0 {
                break;
            }
        }
    }

    #[test]
    fn canonical_varint_boundaries() {
        assert_eq!(canonical_varint_len(0), 1);
        assert_eq!(canonical_varint_len(127), 1);
        assert_eq!(canonical_varint_len(128), 2);
        assert_eq!(parse_varint(&[0x80, 0x01], 0), Ok((128, 2)));
        assert!(parse_varint(&[0x80, 0x00], 0).is_err());
        assert!(parse_varint(&[0x80], 0).is_err());
    }

    #[test]
    fn additive_unknown_scalar_and_bytes_fields_are_skipped() {
        let payload = [0xa0, 0x06, 0x01, 0xaa, 0x06, 0x03, b'a', b'b', b'c'];
        assert!(validate_checked_in("eutheto.worker.v1.HandshakeRequest", &payload).is_ok());
    }

    #[test]
    fn reserved_and_malformed_unknown_fields_are_rejected() {
        assert!(matches!(
            validate_checked_in("eutheto.worker.v1.SolveParameters", &[0x08, 0x00]),
            Err(ProtocolFault::Wire(fault))
                if fault.violation == WireViolation::ReservedField(1)
        ));
        assert!(matches!(
            validate_checked_in(
                "eutheto.worker.v1.HandshakeRequest",
                &[0xc0, 0xa3, 0x09, 0x01],
            ),
            Err(ProtocolFault::Wire(fault))
                if fault.violation == WireViolation::ReservedField(19_000)
        ));
        for payload in [&[0xaa, 0x06, 0x80, 0x00][..], &[0xaa, 0x06, 0x05, 0x01][..]] {
            assert!(validate_checked_in("eutheto.worker.v1.HandshakeRequest", payload).is_err());
        }
    }

    #[test]
    fn packed_capability_payload_stops_at_repeated_cap() {
        let mut payload = vec![0x4a];
        append_varint(&mut payload, 1 << 20);
        payload.resize(payload.len() + (1 << 20), 1);
        assert!(matches!(
            validate_checked_in("eutheto.worker.v1.HandshakeSuccess", &payload),
            Err(ProtocolFault::Wire(fault))
                if matches!(fault.violation, WireViolation::RepeatedCount { cap: 64, .. })
        ));
    }

    #[test]
    fn packed_assumptions_stop_at_field_cap_plus_one() {
        let count = 100_001;
        let mut payload = vec![0x7a];
        append_varint(&mut payload, count);
        payload.resize(payload.len() + count, 1);
        assert!(matches!(
            validate_checked_in("eutheto.worker.v1.Finished", &payload),
            Err(ProtocolFault::Wire(fault))
                if matches!(
                    fault.violation,
                    WireViolation::RepeatedCount { cap: 100_000, .. }
                )
        ));
    }
}
