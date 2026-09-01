use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use prost::Message;
use prost_types::field_descriptor_proto::{Label, Type};
use prost_types::{DescriptorProto, EnumDescriptorProto, FileDescriptorSet};
use serde::Deserialize;

use crate::protocol_generate::GeneratedOutput;

// These allowances apply only to generated upstream code emitted by prost-build.
#[allow(clippy::doc_markdown, clippy::trivially_copy_pass_by_ref)]
pub(crate) mod wire {
    include!(concat!(env!("OUT_DIR"), "/eutheto.worker.v1.rs"));
}

const EXPECTED_MESSAGES: &[&str] = &[
    "ParentFrame",
    "WorkerFrame",
    "HandshakeRequest",
    "HandshakeResponse",
    "HandshakeSuccess",
    "HandshakeError",
    "SolveRequest",
    "SolveParameters",
    "ProjectionRequest",
    "ResourceLimits",
    "Started",
    "Progress",
    "Incumbent",
    "ProjectedCandidate",
    "ProjectedValue",
    "Finished",
    "WorkerError",
];
const EXPECTED_ENUMS: &[&str] = &[
    "HandshakeErrorCode",
    "Capability",
    "ProgressKind",
    "WorkerSolveStatus",
    "TerminationReason",
    "WorkerErrorCode",
];
const EXPECTED_FIXTURES: &[&str] = &[
    "finished",
    "handshake-error",
    "handshake-request",
    "handshake-response",
    "incumbent",
    "progress",
    "solve-request",
    "started",
    "worker-error",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionContract {
    compatibility: Compatibility,
    field_limits: BTreeMap<String, FieldLimit>,
    frame_classes: BTreeMap<String, FrameClass>,
    framing: Framing,
    limits: Limits,
    package: String,
    protocol: String,
    version: ProtocolVersion,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Compatibility {
    accepted_protocol_majors: Vec<u32>,
    unknown_major_action: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldLimit {
    max_bytes: Option<usize>,
    max_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameClass {
    max_payload_bytes: usize,
    routes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Framing {
    length_prefix_bytes: usize,
    length_prefix_order: String,
    min_payload_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    events_per_second: usize,
    events_per_session: usize,
    frames_per_session: usize,
    max_nesting_depth: usize,
    max_repeated_field_items: usize,
    max_stderr_bytes: usize,
    max_string_bytes: usize,
    max_worker_threads: usize,
    total_session_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtocolVersion {
    major: u32,
    minor: u32,
}

pub fn verify(repo_root: &Path) -> Result<()> {
    let contract = read_contract(repo_root)?;
    validate_policy(&contract)?;
    let descriptor_bytes = fs::read(repo_root.join(crate::protocol_generate::DESCRIPTOR_PATH))
        .context("failed to read checked-in worker protocol descriptor")?;
    let descriptor = FileDescriptorSet::decode(descriptor_bytes.as_slice())
        .context("checked-in worker protocol descriptor is malformed")?;
    validate_descriptor(&descriptor, &contract)?;
    validate_fixtures(repo_root, &contract)?;
    let expected = crate::protocol_generate::generated_files(repo_root)?;
    crate::protocol_generate::reject_unexpected(repo_root, &expected)?;
    compare_generated_protocol_outputs(repo_root, &expected)?;
    println!(
        "verified worker protocol schema, policy, descriptor, bindings, and {} fixture pair(s)",
        EXPECTED_FIXTURES.len()
    );
    Ok(())
}

fn read_contract(repo_root: &Path) -> Result<VersionContract> {
    let path = repo_root.join("protocol/version.json");
    let source = fs::read_to_string(&path)
        .with_context(|| format!("failed to read protocol policy {}", path.display()))?;
    serde_json::from_str(&source)
        .with_context(|| format!("invalid protocol policy JSON in {}", path.display()))
}

#[allow(clippy::too_many_lines)]
fn validate_policy(contract: &VersionContract) -> Result<()> {
    ensure!(
        contract.protocol == "eutheto.solver-worker",
        "unexpected protocol identity"
    );
    ensure!(
        contract.package == "eutheto.worker.v1",
        "unexpected protobuf package"
    );
    ensure!(contract.version.major == 1, "protocol major must remain 1");
    ensure!(
        contract.version.minor == 0,
        "initial v1 protocol minor must remain 0"
    );
    ensure!(
        contract.compatibility.accepted_protocol_majors == [contract.version.major],
        "accepted protocol majors must contain exactly the current major"
    );
    ensure!(
        contract.compatibility.unknown_major_action == "typed_handshake_error_then_close",
        "unknown major behavior must be a typed handshake error followed by close"
    );
    ensure!(
        contract.framing.length_prefix_bytes == 4,
        "length prefix must be four bytes"
    );
    ensure!(
        contract.framing.length_prefix_order == "big-endian",
        "length prefix must be big-endian"
    );
    ensure!(
        contract.framing.min_payload_bytes == 1,
        "empty transport payloads are forbidden"
    );

    let exact_frame_classes: &[(&str, usize, &[&str])] = &[
        (
            "handshake",
            1_048_576,
            &[
                "ParentFrame.handshake_request",
                "WorkerFrame.handshake_response",
            ],
        ),
        ("solve_request", 268_435_456, &["ParentFrame.solve_request"]),
        (
            "worker_event",
            16_777_216,
            &[
                "WorkerFrame.started",
                "WorkerFrame.progress",
                "WorkerFrame.incumbent",
                "WorkerFrame.finished",
                "WorkerFrame.error",
            ],
        ),
    ];
    ensure!(
        contract.frame_classes.len() == exact_frame_classes.len(),
        "unexpected frame class inventory"
    );
    for (name, max_payload_bytes, routes) in exact_frame_classes {
        let class = contract
            .frame_classes
            .get(*name)
            .with_context(|| format!("missing frame class `{name}`"))?;
        ensure!(
            class.max_payload_bytes == *max_payload_bytes,
            "incorrect `{name}` frame cap"
        );
        ensure!(
            class
                .routes
                .iter()
                .map(String::as_str)
                .eq(routes.iter().copied()),
            "incorrect `{name}` frame routes"
        );
    }
    ensure!(
        contract.limits.max_stderr_bytes == 4_194_304,
        "stderr cap must be exactly 4 MiB"
    );
    for (name, actual, expected) in [
        ("events_per_second", contract.limits.events_per_second, 64),
        (
            "events_per_session",
            contract.limits.events_per_session,
            4_096,
        ),
        (
            "frames_per_session",
            contract.limits.frames_per_session,
            4_099,
        ),
        ("max_nesting_depth", contract.limits.max_nesting_depth, 8),
        (
            "max_repeated_field_items",
            contract.limits.max_repeated_field_items,
            100_000,
        ),
        ("max_string_bytes", contract.limits.max_string_bytes, 4_096),
        (
            "max_worker_threads",
            contract.limits.max_worker_threads,
            10_000,
        ),
        (
            "total_session_bytes",
            contract.limits.total_session_bytes,
            536_870_912,
        ),
    ] {
        ensure!(
            actual == expected,
            "policy ceiling `{name}` must be exactly {expected}"
        );
    }
    validate_exact_field_limits(contract)?;
    ensure!(
        contract.limits.events_per_session <= contract.limits.frames_per_session,
        "event count cannot exceed total frame count"
    );
    ensure!(
        contract.limits.total_session_bytes
            >= contract.frame_classes["solve_request"].max_payload_bytes,
        "session byte ceiling cannot be smaller than one solve request"
    );
    Ok(())
}
#[allow(clippy::too_many_lines)]
fn validate_exact_field_limits(contract: &VersionContract) -> Result<()> {
    let expected: &[(&str, Option<usize>, Option<usize>)] = &[
        (
            "eutheto.worker.v1.Finished.applied_parameters_sha256",
            Some(32),
            None,
        ),
        (
            "eutheto.worker.v1.Finished.best_bound_values",
            None,
            Some(64),
        ),
        (
            "eutheto.worker.v1.Finished.model_fingerprint",
            Some(32),
            None,
        ),
        (
            "eutheto.worker.v1.Finished.objective_values",
            None,
            Some(64),
        ),
        ("eutheto.worker.v1.Finished.request_id", Some(64), None),
        (
            "eutheto.worker.v1.Finished.sufficient_assumptions",
            None,
            Some(100_000),
        ),
        ("eutheto.worker.v1.HandshakeError.message", Some(512), None),
        (
            "eutheto.worker.v1.HandshakeRequest.core_version",
            Some(64),
            None,
        ),
        (
            "eutheto.worker.v1.HandshakeRequest.expected_backend_id",
            Some(128),
            None,
        ),
        (
            "eutheto.worker.v1.HandshakeRequest.required_capabilities",
            None,
            Some(64),
        ),
        (
            "eutheto.worker.v1.HandshakeSuccess.adapter_version",
            Some(64),
            None,
        ),
        (
            "eutheto.worker.v1.HandshakeSuccess.backend_id",
            Some(128),
            None,
        ),
        (
            "eutheto.worker.v1.HandshakeSuccess.capabilities",
            None,
            Some(64),
        ),
        (
            "eutheto.worker.v1.HandshakeSuccess.manifest_sha256",
            Some(32),
            None,
        ),
        (
            "eutheto.worker.v1.HandshakeSuccess.ortools_version",
            Some(64),
            None,
        ),
        (
            "eutheto.worker.v1.HandshakeSuccess.worker_identity",
            Some(256),
            None,
        ),
        (
            "eutheto.worker.v1.HandshakeSuccess.worker_version",
            Some(64),
            None,
        ),
        (
            "eutheto.worker.v1.Incumbent.best_bound_values",
            None,
            Some(64),
        ),
        (
            "eutheto.worker.v1.Incumbent.objective_values",
            None,
            Some(64),
        ),
        ("eutheto.worker.v1.Incumbent.request_id", Some(64), None),
        (
            "eutheto.worker.v1.Progress.best_bound_values",
            None,
            Some(64),
        ),
        (
            "eutheto.worker.v1.Progress.objective_values",
            None,
            Some(64),
        ),
        ("eutheto.worker.v1.Progress.request_id", Some(64), None),
        (
            "eutheto.worker.v1.ProjectedCandidate.values",
            None,
            Some(100_000),
        ),
        (
            "eutheto.worker.v1.SolveRequest.cp_model_proto",
            Some(267_386_880),
            None,
        ),
        (
            "eutheto.worker.v1.SolveRequest.model_fingerprint",
            Some(32),
            None,
        ),
        (
            "eutheto.worker.v1.SolveRequest.projections",
            None,
            Some(100_000),
        ),
        ("eutheto.worker.v1.SolveRequest.request_id", Some(64), None),
        (
            "eutheto.worker.v1.Started.model_fingerprint",
            Some(32),
            None,
        ),
        ("eutheto.worker.v1.Started.request_id", Some(64), None),
        ("eutheto.worker.v1.WorkerError.message", Some(512), None),
        ("eutheto.worker.v1.WorkerError.request_id", Some(64), None),
    ];
    ensure!(
        contract.field_limits.len() == expected.len(),
        "field-limit inventory changed"
    );
    for (field, max_bytes, max_count) in expected {
        let actual = contract
            .field_limits
            .get(*field)
            .with_context(|| format!("missing field limit `{field}`"))?;
        ensure!(
            actual.max_bytes == *max_bytes && actual.max_count == *max_count,
            "field limit `{field}` changed"
        );
    }
    Ok(())
}

fn validate_descriptor(descriptor: &FileDescriptorSet, contract: &VersionContract) -> Result<()> {
    ensure!(
        descriptor.file.len() == 1,
        "protocol descriptor must contain exactly one file"
    );
    let file = &descriptor.file[0];
    ensure!(
        file.name.as_deref() == Some("solver-worker.proto"),
        "unexpected descriptor filename"
    );
    ensure!(
        file.package.as_deref() == Some(contract.package.as_str()),
        "descriptor package disagrees with policy"
    );
    ensure!(
        file.syntax.as_deref() == Some("proto3"),
        "protocol schema must use proto3"
    );
    let messages = file
        .message_type
        .iter()
        .map(|message| required_name(message.name.as_deref(), "message"))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        messages == EXPECTED_MESSAGES,
        "top-level protocol message inventory changed"
    );
    let enums = file
        .enum_type
        .iter()
        .map(|enumeration| required_name(enumeration.name.as_deref(), "enum"))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        enums == EXPECTED_ENUMS,
        "top-level protocol enum inventory changed"
    );
    validate_enum_allocations(&file.enum_type)?;
    validate_stable_tags(&file.message_type)?;
    validate_policy_fields(file.message_type.as_slice(), contract)?;
    validate_frame_routes(file.message_type.as_slice(), contract)?;
    Ok(())
}

#[allow(clippy::too_many_lines, clippy::type_complexity)]
fn validate_enum_allocations(enums: &[EnumDescriptorProto]) -> Result<()> {
    let expected: &[(&str, &[(&str, i32)], &[(i32, i32)])] = &[
        (
            "HandshakeErrorCode",
            &[
                ("HANDSHAKE_ERROR_CODE_UNSPECIFIED", 0),
                ("HANDSHAKE_ERROR_CODE_UNSUPPORTED_PROTOCOL_MAJOR", 1),
                ("HANDSHAKE_ERROR_CODE_UNSUPPORTED_PROTOCOL_MINOR", 2),
                ("HANDSHAKE_ERROR_CODE_UNEXPECTED_BACKEND", 3),
                ("HANDSHAKE_ERROR_CODE_MISSING_CAPABILITY", 4),
                ("HANDSHAKE_ERROR_CODE_INVALID_VERSION", 5),
                ("HANDSHAKE_ERROR_CODE_MANIFEST_MISMATCH", 6),
            ],
            &[(7, 31)],
        ),
        (
            "Capability",
            &[
                ("CAPABILITY_UNSPECIFIED", 0),
                ("CAPABILITY_CP_SAT", 1),
                ("CAPABILITY_INTERMEDIATE_SOLUTIONS", 2),
                ("CAPABILITY_PROGRESS", 3),
                ("CAPABILITY_SOLUTION_PROJECTION", 4),
                ("CAPABILITY_OBJECTIVE_BOUNDS", 5),
                ("CAPABILITY_SOLUTION_STATS", 6),
                ("CAPABILITY_SUFFICIENT_ASSUMPTIONS", 7),
                ("CAPABILITY_DETERMINISTIC_TIME", 8),
            ],
            &[(9, 31)],
        ),
        (
            "ProgressKind",
            &[
                ("PROGRESS_KIND_UNSPECIFIED", 0),
                ("PROGRESS_KIND_PRESOLVE", 1),
                ("PROGRESS_KIND_SEARCH", 2),
                ("PROGRESS_KIND_BOUND_IMPROVED", 3),
            ],
            &[(4, 15)],
        ),
        (
            "WorkerSolveStatus",
            &[
                ("WORKER_SOLVE_STATUS_UNSPECIFIED", 0),
                ("WORKER_SOLVE_STATUS_OPTIMAL", 1),
                ("WORKER_SOLVE_STATUS_FEASIBLE", 2),
                ("WORKER_SOLVE_STATUS_INFEASIBLE", 3),
                ("WORKER_SOLVE_STATUS_NO_SOLUTION", 4),
                ("WORKER_SOLVE_STATUS_INVALID_MODEL", 5),
                ("WORKER_SOLVE_STATUS_CANCELLED", 6),
                ("WORKER_SOLVE_STATUS_FAILED", 7),
            ],
            &[(8, 31)],
        ),
        (
            "TerminationReason",
            &[
                ("TERMINATION_REASON_UNSPECIFIED", 0),
                ("TERMINATION_REASON_OPTIMAL", 1),
                ("TERMINATION_REASON_INFEASIBLE", 2),
                ("TERMINATION_REASON_TIME_LIMIT", 3),
                ("TERMINATION_REASON_SOLUTION_LIMIT", 4),
                ("TERMINATION_REASON_MEMORY_LIMIT", 5),
                ("TERMINATION_REASON_CANCELLED", 6),
                ("TERMINATION_REASON_INVALID_MODEL", 7),
                ("TERMINATION_REASON_INTERNAL_ERROR", 8),
                ("TERMINATION_REASON_UNKNOWN", 9),
            ],
            &[(10, 31)],
        ),
        (
            "WorkerErrorCode",
            &[
                ("WORKER_ERROR_CODE_UNSPECIFIED", 0),
                ("WORKER_ERROR_CODE_MALFORMED_FRAME", 1),
                ("WORKER_ERROR_CODE_PROTOCOL_VIOLATION", 2),
                ("WORKER_ERROR_CODE_UNSUPPORTED_MODEL", 3),
                ("WORKER_ERROR_CODE_INVALID_MODEL", 4),
                ("WORKER_ERROR_CODE_INVALID_PARAMETERS", 5),
                ("WORKER_ERROR_CODE_RESOURCE_LIMIT", 6),
                ("WORKER_ERROR_CODE_ORTOOLS_INITIALIZATION", 7),
                ("WORKER_ERROR_CODE_INTERNAL", 8),
            ],
            &[(9, 31)],
        ),
    ];
    ensure!(
        expected.len() == EXPECTED_ENUMS.len(),
        "stable enum inventory must cover every enum"
    );
    for (name, values, reserved_ranges) in expected {
        let enumeration = enums
            .iter()
            .find(|enumeration| enumeration.name.as_deref() == Some(*name))
            .with_context(|| format!("descriptor is missing enum `{name}`"))?;
        let actual_values = enumeration
            .value
            .iter()
            .map(|value| {
                (
                    value.name.as_deref().unwrap_or("<unnamed>"),
                    value.number.unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        ensure!(
            actual_values == *values,
            "enum allocation changed for {name}"
        );
        // EnumDescriptorProto reserved-range ends are inclusive; message
        // reserved-range ends are exclusive.
        let actual_reserved = enumeration
            .reserved_range
            .iter()
            .map(|range| {
                (
                    range.start.unwrap_or_default(),
                    range.end.unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        ensure!(
            actual_reserved == *reserved_ranges,
            "enum reserved ranges changed for {name}"
        );
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ExpectedField {
    name: &'static str,
    number: i32,
    kind: Type,
    label: Label,
    type_name: Option<&'static str>,
    oneof_index: Option<i32>,
    proto3_optional: bool,
}

const fn scalar(name: &'static str, number: i32, kind: Type) -> ExpectedField {
    ExpectedField {
        name,
        number,
        kind,
        label: Label::Optional,
        type_name: None,
        oneof_index: None,
        proto3_optional: false,
    }
}

const fn repeated(name: &'static str, number: i32, kind: Type) -> ExpectedField {
    ExpectedField {
        label: Label::Repeated,
        ..scalar(name, number, kind)
    }
}

const fn typed(
    name: &'static str,
    number: i32,
    kind: Type,
    type_name: &'static str,
) -> ExpectedField {
    ExpectedField {
        type_name: Some(type_name),
        ..scalar(name, number, kind)
    }
}

const fn repeated_typed(
    name: &'static str,
    number: i32,
    kind: Type,
    type_name: &'static str,
) -> ExpectedField {
    ExpectedField {
        label: Label::Repeated,
        type_name: Some(type_name),
        ..scalar(name, number, kind)
    }
}

const fn oneof_typed(
    name: &'static str,
    number: i32,
    kind: Type,
    type_name: &'static str,
    oneof_index: i32,
) -> ExpectedField {
    ExpectedField {
        type_name: Some(type_name),
        oneof_index: Some(oneof_index),
        ..scalar(name, number, kind)
    }
}

const fn proto3_optional(
    name: &'static str,
    number: i32,
    kind: Type,
    oneof_index: i32,
) -> ExpectedField {
    ExpectedField {
        oneof_index: Some(oneof_index),
        proto3_optional: true,
        ..scalar(name, number, kind)
    }
}

const fn proto3_optional_typed(
    name: &'static str,
    number: i32,
    kind: Type,
    type_name: &'static str,
    oneof_index: i32,
) -> ExpectedField {
    ExpectedField {
        type_name: Some(type_name),
        oneof_index: Some(oneof_index),
        proto3_optional: true,
        ..scalar(name, number, kind)
    }
}

#[allow(clippy::too_many_lines)]
fn validate_field_signatures(messages: &[DescriptorProto]) -> Result<()> {
    let expected: &[(&str, &[ExpectedField])] = &[
        (
            "ParentFrame",
            &[
                oneof_typed(
                    "handshake_request",
                    1,
                    Type::Message,
                    ".eutheto.worker.v1.HandshakeRequest",
                    0,
                ),
                oneof_typed(
                    "solve_request",
                    2,
                    Type::Message,
                    ".eutheto.worker.v1.SolveRequest",
                    0,
                ),
            ],
        ),
        (
            "WorkerFrame",
            &[
                oneof_typed(
                    "handshake_response",
                    1,
                    Type::Message,
                    ".eutheto.worker.v1.HandshakeResponse",
                    0,
                ),
                oneof_typed("started", 2, Type::Message, ".eutheto.worker.v1.Started", 0),
                oneof_typed(
                    "progress",
                    3,
                    Type::Message,
                    ".eutheto.worker.v1.Progress",
                    0,
                ),
                oneof_typed(
                    "incumbent",
                    4,
                    Type::Message,
                    ".eutheto.worker.v1.Incumbent",
                    0,
                ),
                oneof_typed(
                    "finished",
                    5,
                    Type::Message,
                    ".eutheto.worker.v1.Finished",
                    0,
                ),
                oneof_typed(
                    "error",
                    6,
                    Type::Message,
                    ".eutheto.worker.v1.WorkerError",
                    0,
                ),
            ],
        ),
        (
            "HandshakeRequest",
            &[
                scalar("protocol_major", 1, Type::Uint32),
                scalar("protocol_minor", 2, Type::Uint32),
                scalar("core_version", 3, Type::String),
                scalar("expected_backend_id", 4, Type::String),
                repeated_typed(
                    "required_capabilities",
                    5,
                    Type::Enum,
                    ".eutheto.worker.v1.Capability",
                ),
            ],
        ),
        (
            "HandshakeResponse",
            &[
                oneof_typed(
                    "success",
                    1,
                    Type::Message,
                    ".eutheto.worker.v1.HandshakeSuccess",
                    0,
                ),
                oneof_typed(
                    "error",
                    2,
                    Type::Message,
                    ".eutheto.worker.v1.HandshakeError",
                    0,
                ),
            ],
        ),
        (
            "HandshakeSuccess",
            &[
                scalar("protocol_major", 1, Type::Uint32),
                scalar("protocol_minor", 2, Type::Uint32),
                scalar("worker_identity", 3, Type::String),
                scalar("worker_version", 4, Type::String),
                scalar("backend_id", 5, Type::String),
                scalar("ortools_version", 6, Type::String),
                scalar("adapter_version", 7, Type::String),
                scalar("manifest_sha256", 8, Type::Bytes),
                repeated_typed(
                    "capabilities",
                    9,
                    Type::Enum,
                    ".eutheto.worker.v1.Capability",
                ),
            ],
        ),
        (
            "HandshakeError",
            &[
                typed(
                    "code",
                    1,
                    Type::Enum,
                    ".eutheto.worker.v1.HandshakeErrorCode",
                ),
                scalar("message", 2, Type::String),
                proto3_optional("supported_protocol_major", 3, Type::Uint32, 0),
                proto3_optional("supported_protocol_minor", 4, Type::Uint32, 1),
            ],
        ),
        (
            "SolveRequest",
            &[
                scalar("request_id", 1, Type::String),
                scalar("cp_model_proto", 2, Type::Bytes),
                typed(
                    "parameters",
                    3,
                    Type::Message,
                    ".eutheto.worker.v1.SolveParameters",
                ),
                repeated_typed(
                    "projections",
                    4,
                    Type::Message,
                    ".eutheto.worker.v1.ProjectionRequest",
                ),
                typed(
                    "resource_limits",
                    5,
                    Type::Message,
                    ".eutheto.worker.v1.ResourceLimits",
                ),
                scalar("model_fingerprint", 6, Type::Bytes),
            ],
        ),
        (
            "SolveParameters",
            &[
                proto3_optional("random_seed", 3, Type::Int32, 0),
                proto3_optional("stop_after_first_feasible", 4, Type::Bool, 1),
                proto3_optional("emit_intermediate_solutions", 5, Type::Bool, 2),
                proto3_optional("log_search_progress", 6, Type::Bool, 3),
                proto3_optional("deterministic_test_profile", 7, Type::Bool, 4),
            ],
        ),
        (
            "ProjectionRequest",
            &[
                scalar("projection_id", 1, Type::Uint64),
                scalar("cp_sat_variable_index", 2, Type::Int32),
            ],
        ),
        (
            "ResourceLimits",
            &[
                scalar("wall_time_millis", 1, Type::Uint64),
                proto3_optional("memory_bytes", 2, Type::Uint64, 0),
                scalar("worker_threads", 3, Type::Uint32),
            ],
        ),
        (
            "Started",
            &[
                scalar("request_id", 1, Type::String),
                scalar("model_fingerprint", 2, Type::Bytes),
            ],
        ),
        (
            "Progress",
            &[
                scalar("request_id", 1, Type::String),
                typed("kind", 2, Type::Enum, ".eutheto.worker.v1.ProgressKind"),
                repeated("objective_values", 3, Type::Double),
                repeated("best_bound_values", 4, Type::Double),
                proto3_optional("wall_time_seconds", 5, Type::Double, 0),
                proto3_optional("deterministic_time", 6, Type::Double, 1),
            ],
        ),
        (
            "Incumbent",
            &[
                scalar("request_id", 1, Type::String),
                typed(
                    "candidate",
                    2,
                    Type::Message,
                    ".eutheto.worker.v1.ProjectedCandidate",
                ),
                repeated("objective_values", 3, Type::Double),
                repeated("best_bound_values", 4, Type::Double),
                proto3_optional("wall_time_seconds", 5, Type::Double, 0),
                proto3_optional("deterministic_time", 6, Type::Double, 1),
            ],
        ),
        (
            "ProjectedCandidate",
            &[repeated_typed(
                "values",
                1,
                Type::Message,
                ".eutheto.worker.v1.ProjectedValue",
            )],
        ),
        (
            "ProjectedValue",
            &[
                scalar("projection_id", 1, Type::Uint64),
                scalar("value", 2, Type::Int64),
            ],
        ),
        (
            "Finished",
            &[
                scalar("request_id", 1, Type::String),
                scalar("raw_cp_sat_status", 2, Type::Int32),
                typed(
                    "status",
                    3,
                    Type::Enum,
                    ".eutheto.worker.v1.WorkerSolveStatus",
                ),
                typed(
                    "termination_reason",
                    4,
                    Type::Enum,
                    ".eutheto.worker.v1.TerminationReason",
                ),
                proto3_optional_typed(
                    "final_candidate",
                    5,
                    Type::Message,
                    ".eutheto.worker.v1.ProjectedCandidate",
                    0,
                ),
                repeated("objective_values", 6, Type::Double),
                repeated("best_bound_values", 7, Type::Double),
                proto3_optional("wall_time_seconds", 8, Type::Double, 1),
                proto3_optional("user_time_seconds", 9, Type::Double, 2),
                proto3_optional("deterministic_time", 10, Type::Double, 3),
                proto3_optional("conflicts", 11, Type::Uint64, 4),
                proto3_optional("branches", 12, Type::Uint64, 5),
                proto3_optional("binary_propagations", 13, Type::Uint64, 6),
                proto3_optional("integer_propagations", 14, Type::Uint64, 7),
                repeated("sufficient_assumptions", 15, Type::Int32),
                scalar("applied_parameters_sha256", 16, Type::Bytes),
                scalar("model_fingerprint", 17, Type::Bytes),
            ],
        ),
        (
            "WorkerError",
            &[
                scalar("request_id", 1, Type::String),
                typed("code", 2, Type::Enum, ".eutheto.worker.v1.WorkerErrorCode"),
                scalar("message", 3, Type::String),
                scalar("retryable", 4, Type::Bool),
            ],
        ),
    ];
    ensure!(
        expected.len() == EXPECTED_MESSAGES.len(),
        "field signature inventory must cover every message"
    );
    for (message_name, expected_fields) in expected {
        let message = find_message(messages, message_name)?;
        ensure!(
            message.field.len() == expected_fields.len(),
            "descriptor message `{message_name}` has an unreviewed field addition or removal"
        );
        for expected_field in *expected_fields {
            let field = message
                .field
                .iter()
                .find(|field| field.name.as_deref() == Some(expected_field.name))
                .with_context(|| {
                    format!(
                        "descriptor message `{message_name}` is missing field `{}`",
                        expected_field.name
                    )
                })?;
            ensure!(
                field.number == Some(expected_field.number)
                    && field.r#type() == expected_field.kind
                    && field.label() == expected_field.label
                    && field.type_name.as_deref() == expected_field.type_name
                    && field.oneof_index == expected_field.oneof_index
                    && field.proto3_optional.unwrap_or(false) == expected_field.proto3_optional,
                "field descriptor signature changed for {message_name}.{}",
                expected_field.name
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_stable_tags(messages: &[DescriptorProto]) -> Result<()> {
    let expected: &[(&str, &[(&str, i32)])] = &[
        (
            "ParentFrame",
            &[("handshake_request", 1), ("solve_request", 2)],
        ),
        (
            "WorkerFrame",
            &[
                ("handshake_response", 1),
                ("started", 2),
                ("progress", 3),
                ("incumbent", 4),
                ("finished", 5),
                ("error", 6),
            ],
        ),
        (
            "HandshakeRequest",
            &[
                ("protocol_major", 1),
                ("protocol_minor", 2),
                ("core_version", 3),
                ("expected_backend_id", 4),
                ("required_capabilities", 5),
            ],
        ),
        ("HandshakeResponse", &[("success", 1), ("error", 2)]),
        (
            "HandshakeSuccess",
            &[
                ("protocol_major", 1),
                ("protocol_minor", 2),
                ("worker_identity", 3),
                ("worker_version", 4),
                ("backend_id", 5),
                ("ortools_version", 6),
                ("adapter_version", 7),
                ("manifest_sha256", 8),
                ("capabilities", 9),
            ],
        ),
        (
            "HandshakeError",
            &[
                ("code", 1),
                ("message", 2),
                ("supported_protocol_major", 3),
                ("supported_protocol_minor", 4),
            ],
        ),
        (
            "SolveRequest",
            &[
                ("request_id", 1),
                ("cp_model_proto", 2),
                ("parameters", 3),
                ("projections", 4),
                ("resource_limits", 5),
                ("model_fingerprint", 6),
            ],
        ),
        (
            "SolveParameters",
            &[
                ("random_seed", 3),
                ("stop_after_first_feasible", 4),
                ("emit_intermediate_solutions", 5),
                ("log_search_progress", 6),
                ("deterministic_test_profile", 7),
            ],
        ),
        (
            "ProjectionRequest",
            &[("projection_id", 1), ("cp_sat_variable_index", 2)],
        ),
        (
            "ResourceLimits",
            &[
                ("wall_time_millis", 1),
                ("memory_bytes", 2),
                ("worker_threads", 3),
            ],
        ),
        ("Started", &[("request_id", 1), ("model_fingerprint", 2)]),
        (
            "Progress",
            &[
                ("request_id", 1),
                ("kind", 2),
                ("objective_values", 3),
                ("best_bound_values", 4),
                ("wall_time_seconds", 5),
                ("deterministic_time", 6),
            ],
        ),
        (
            "Incumbent",
            &[
                ("request_id", 1),
                ("candidate", 2),
                ("objective_values", 3),
                ("best_bound_values", 4),
                ("wall_time_seconds", 5),
                ("deterministic_time", 6),
            ],
        ),
        ("ProjectedCandidate", &[("values", 1)]),
        ("ProjectedValue", &[("projection_id", 1), ("value", 2)]),
        (
            "Finished",
            &[
                ("request_id", 1),
                ("raw_cp_sat_status", 2),
                ("status", 3),
                ("termination_reason", 4),
                ("final_candidate", 5),
                ("objective_values", 6),
                ("best_bound_values", 7),
                ("wall_time_seconds", 8),
                ("user_time_seconds", 9),
                ("deterministic_time", 10),
                ("conflicts", 11),
                ("branches", 12),
                ("binary_propagations", 13),
                ("integer_propagations", 14),
                ("sufficient_assumptions", 15),
                ("applied_parameters_sha256", 16),
                ("model_fingerprint", 17),
            ],
        ),
        (
            "WorkerError",
            &[
                ("request_id", 1),
                ("code", 2),
                ("message", 3),
                ("retryable", 4),
            ],
        ),
    ];
    ensure!(
        expected.len() == EXPECTED_MESSAGES.len(),
        "stable tag inventory must cover every message"
    );
    for (message_name, fields) in expected {
        let message = find_message(messages, message_name)?;
        for (field_name, field_number) in *fields {
            let field = message
                .field
                .iter()
                .find(|field| field.name.as_deref() == Some(*field_name))
                .with_context(|| {
                    format!("descriptor message `{message_name}` is missing field `{field_name}`")
                })?;
            ensure!(
                field.number == Some(*field_number),
                "field tag changed for {message_name}.{field_name}"
            );
        }
    }
    validate_field_signatures(messages)?;
    let expected_reserved: &[(&str, &[(i32, i32)])] = &[
        ("ParentFrame", &[(3, 16)]),
        ("WorkerFrame", &[(7, 16)]),
        ("HandshakeRequest", &[(6, 16)]),
        ("HandshakeResponse", &[(3, 16)]),
        ("HandshakeSuccess", &[(10, 32)]),
        ("HandshakeError", &[(5, 16)]),
        ("SolveRequest", &[(7, 32)]),
        ("SolveParameters", &[(1, 2), (2, 3), (8, 32)]),
        ("ProjectionRequest", &[(3, 16)]),
        ("ResourceLimits", &[(4, 16)]),
        ("Started", &[(3, 16)]),
        ("Progress", &[(7, 16)]),
        ("Incumbent", &[(7, 16)]),
        ("ProjectedCandidate", &[(2, 16)]),
        ("ProjectedValue", &[(3, 16)]),
        ("Finished", &[(18, 32)]),
        ("WorkerError", &[(5, 16)]),
    ];
    ensure!(
        expected_reserved.len() == EXPECTED_MESSAGES.len(),
        "reserved-range inventory must cover every message"
    );
    for (message_name, ranges) in expected_reserved {
        let actual = find_message(messages, message_name)?
            .reserved_range
            .iter()
            .map(|range| {
                (
                    range.start.unwrap_or_default(),
                    range.end.unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        ensure!(
            actual == *ranges,
            "reserved tag ranges changed for {message_name}"
        );
    }
    Ok(())
}

fn validate_policy_fields(messages: &[DescriptorProto], contract: &VersionContract) -> Result<()> {
    for (qualified, limit) in &contract.field_limits {
        ensure!(
            limit.max_bytes.is_some() ^ limit.max_count.is_some(),
            "field limit `{qualified}` must set exactly one ceiling"
        );
        let prefix = format!("{}.", contract.package);
        let local = qualified.strip_prefix(&prefix).with_context(|| {
            format!("field limit `{qualified}` is outside the protocol package")
        })?;
        let (message_name, field_name) = local
            .split_once('.')
            .with_context(|| format!("invalid fully-qualified field limit `{qualified}`"))?;
        let field = find_message(messages, message_name)?
            .field
            .iter()
            .find(|field| field.name.as_deref() == Some(field_name))
            .with_context(|| format!("field limit references unknown field `{qualified}`"))?;
        if let Some(max_bytes) = limit.max_bytes {
            ensure!(
                max_bytes > 0,
                "byte ceiling for `{qualified}` must be positive"
            );
            ensure!(
                matches!(field.r#type(), Type::String | Type::Bytes),
                "byte ceiling references non-string/bytes field `{qualified}`"
            );
            if field.r#type() == Type::String {
                ensure!(
                    max_bytes <= contract.limits.max_string_bytes,
                    "string field ceiling exceeds global ceiling for `{qualified}`"
                );
            }
        }
        if let Some(max_count) = limit.max_count {
            ensure!(
                max_count > 0 && max_count <= contract.limits.max_repeated_field_items,
                "repeated field ceiling is invalid for `{qualified}`"
            );
            ensure!(
                field.label() == Label::Repeated,
                "count ceiling references non-repeated field `{qualified}`"
            );
        }
    }
    Ok(())
}

fn validate_frame_routes(messages: &[DescriptorProto], contract: &VersionContract) -> Result<()> {
    let mut routes = BTreeSet::new();
    for class in contract.frame_classes.values() {
        for route in &class.routes {
            ensure!(
                routes.insert(route.as_str()),
                "frame route `{route}` is assigned more than once"
            );
            let (message_name, field_name) = route
                .split_once('.')
                .with_context(|| format!("invalid frame route `{route}`"))?;
            ensure!(
                matches!(message_name, "ParentFrame" | "WorkerFrame"),
                "frame route must begin at a top-level envelope"
            );
            ensure!(
                find_message(messages, message_name)?
                    .field
                    .iter()
                    .any(|field| field.name.as_deref() == Some(field_name)),
                "frame route `{route}` does not exist in the descriptor"
            );
        }
    }
    ensure!(
        routes.len() == 8,
        "every parent/worker frame variant must have exactly one frame class"
    );
    Ok(())
}

fn validate_fixtures(repo_root: &Path, contract: &VersionContract) -> Result<()> {
    let root = repo_root.join("protocol/golden");
    let entries = fs::read_dir(&root)
        .with_context(|| format!("failed to read protocol fixtures {}", root.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut json_names = BTreeSet::new();
    let mut frame_names = BTreeSet::new();
    for entry in entries {
        ensure!(
            entry.file_type()?.is_file(),
            "protocol fixture entries must be regular files"
        );
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("protocol fixture filename is not UTF-8"))?;
        if let Some(stem) = name.strip_suffix(".json") {
            let source = fs::read_to_string(entry.path())?;
            let _: serde_json::Value = serde_json::from_str(&source)
                .with_context(|| format!("invalid fixture JSON `{name}`"))?;
            json_names.insert(stem.to_owned());
        } else if let Some(stem) = name.strip_suffix(".frame.hex") {
            let bytes = decode_hex(&fs::read_to_string(entry.path())?)?;
            ensure!(bytes.len() >= 5, "fixture frame `{name}` is too short");
            let prefix = <[u8; 4]>::try_from(&bytes[..4])
                .context("fixture frame prefix is not four bytes")?;
            let declared = usize::try_from(u32::from_be_bytes(prefix))
                .context("fixture length does not fit usize")?;
            let class = fixture_frame_class(stem)?;
            ensure!(
                declared <= contract.frame_classes[class].max_payload_bytes,
                "fixture `{name}` exceeds its frame class cap"
            );
            if matches!(stem, "handshake-request" | "solve-request") {
                wire::ParentFrame::decode(&bytes[4..])
                    .with_context(|| format!("fixture `{name}` is not a ParentFrame"))?;
            } else {
                wire::WorkerFrame::decode(&bytes[4..])
                    .with_context(|| format!("fixture `{name}` is not a WorkerFrame"))?;
            }
            frame_names.insert(stem.to_owned());
        } else {
            bail!("unsupported protocol fixture file `{name}`");
        }
    }
    let expected = EXPECTED_FIXTURES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();
    ensure!(
        json_names == expected && frame_names == expected,
        "protocol fixture pair inventory changed"
    );
    Ok(())
}

fn fixture_frame_class(name: &str) -> Result<&'static str> {
    match name {
        "handshake-error" | "handshake-request" | "handshake-response" => Ok("handshake"),
        "solve-request" => Ok("solve_request"),
        "started" | "progress" | "incumbent" | "finished" | "worker-error" => Ok("worker_event"),
        _ => bail!("unknown fixture frame class for `{name}`"),
    }
}

fn compare_generated_protocol_outputs(
    repo_root: &Path,
    expected: &[GeneratedOutput],
) -> Result<()> {
    let mut drifted = Vec::new();
    for (relative, bytes) in expected.iter().filter(|(path, _)| {
        path.starts_with("protocol/") || path == crate::protocol_generate::RUST_BINDING_PATH
    }) {
        match fs::read(repo_root.join(relative)) {
            Ok(actual) if actual == *bytes => {}
            Ok(_) | Err(_) => drifted.push(relative.as_str()),
        }
    }
    ensure!(
        drifted.is_empty(),
        "generated protocol outputs are stale: {}",
        drifted.join(", ")
    );
    Ok(())
}

fn find_message<'a>(messages: &'a [DescriptorProto], name: &str) -> Result<&'a DescriptorProto> {
    messages
        .iter()
        .find(|message| message.name.as_deref() == Some(name))
        .with_context(|| format!("descriptor is missing message `{name}`"))
}

fn required_name<'a>(name: Option<&'a str>, kind: &str) -> Result<&'a str> {
    name.with_context(|| format!("descriptor contains unnamed {kind}"))
}

fn decode_hex(source: &str) -> Result<Vec<u8>> {
    let compact = source
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    ensure!(
        compact.len().is_multiple_of(2),
        "hex fixture has an odd digit count"
    );
    compact
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => bail!("invalid hexadecimal digit"),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use prost::Message;
    use prost_types::FileDescriptorSet;

    use super::{read_contract, validate_descriptor, validate_fixtures, validate_policy};

    #[test]
    fn checked_in_descriptor_preserves_package_inventory_and_field_tags() -> Result<()> {
        let root = crate::repository_root()?;
        let contract = read_contract(&root)?;
        let bytes = std::fs::read(root.join(crate::protocol_generate::DESCRIPTOR_PATH))?;
        let descriptor = FileDescriptorSet::decode(bytes.as_slice())?;
        validate_descriptor(&descriptor, &contract)?;
        Ok(())
    }

    #[test]
    fn descriptor_field_type_presence_and_inventory_drift_is_rejected() -> Result<()> {
        let root = crate::repository_root()?;
        let contract = read_contract(&root)?;
        let bytes = std::fs::read(root.join(crate::protocol_generate::DESCRIPTOR_PATH))?;

        let mut wrong_type = FileDescriptorSet::decode(bytes.as_slice())?;
        let Some(file) = wrong_type.file.first_mut() else {
            anyhow::bail!("protocol descriptor has no file");
        };
        let Some(finished) = file
            .message_type
            .iter_mut()
            .find(|message| message.name.as_deref() == Some("Finished"))
        else {
            anyhow::bail!("protocol descriptor has no Finished message");
        };
        let Some(raw_status) = finished
            .field
            .iter_mut()
            .find(|field| field.name.as_deref() == Some("raw_cp_sat_status"))
        else {
            anyhow::bail!("Finished has no raw_cp_sat_status field");
        };
        raw_status.r#type = Some(prost_types::field_descriptor_proto::Type::Sint32 as i32);
        assert!(validate_descriptor(&wrong_type, &contract).is_err());

        let mut wrong_presence = FileDescriptorSet::decode(bytes.as_slice())?;
        let Some(file) = wrong_presence.file.first_mut() else {
            anyhow::bail!("protocol descriptor has no file");
        };
        let Some(parameters) = file
            .message_type
            .iter_mut()
            .find(|message| message.name.as_deref() == Some("SolveParameters"))
        else {
            anyhow::bail!("protocol descriptor has no SolveParameters message");
        };
        let Some(random_seed) = parameters
            .field
            .iter_mut()
            .find(|field| field.name.as_deref() == Some("random_seed"))
        else {
            anyhow::bail!("SolveParameters has no random_seed field");
        };
        random_seed.oneof_index = None;
        random_seed.proto3_optional = Some(false);
        assert!(validate_descriptor(&wrong_presence, &contract).is_err());

        let mut unreviewed_addition = FileDescriptorSet::decode(bytes.as_slice())?;
        let Some(file) = unreviewed_addition.file.first_mut() else {
            anyhow::bail!("protocol descriptor has no file");
        };
        let Some(parent_frame) = file
            .message_type
            .iter_mut()
            .find(|message| message.name.as_deref() == Some("ParentFrame"))
        else {
            anyhow::bail!("protocol descriptor has no ParentFrame message");
        };
        parent_frame.field.push(prost_types::FieldDescriptorProto {
            name: Some("future_extension".to_owned()),
            number: Some(16),
            label: Some(prost_types::field_descriptor_proto::Label::Optional as i32),
            r#type: Some(prost_types::field_descriptor_proto::Type::String as i32),
            ..prost_types::FieldDescriptorProto::default()
        });
        assert!(validate_descriptor(&unreviewed_addition, &contract).is_err());
        Ok(())
    }

    #[test]
    fn policy_metadata_and_canonical_fixtures_agree() -> Result<()> {
        let root = crate::repository_root()?;
        let contract = read_contract(&root)?;
        validate_policy(&contract)?;
        validate_fixtures(&root, &contract)?;
        Ok(())
    }
}
