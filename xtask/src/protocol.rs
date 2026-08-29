use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail, ensure};
use prost::Message;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionContract {
    compatibility: Compatibility,
    framing: Framing,
    limits: Limits,
    protocol: String,
    wire_version: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Compatibility {
    accepted_wire_versions: Vec<u32>,
    unknown_version_action: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Framing {
    length_prefix_bytes: usize,
    length_prefix_order: String,
    max_payload_bytes: usize,
    min_payload_bytes: usize,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    capabilities_per_handshake: usize,
    error_message_bytes: usize,
    frames_per_session: usize,
    manifest_sha256_bytes: usize,
    nesting_depth: usize,
    request_id_bytes: usize,
    worker_identity_bytes: usize,
    worker_version_bytes: usize,
}

const PHASE_00_MAX_PAYLOAD_BYTES: usize = 65_536;

const PHASE_00_LIMIT_BOUNDS: Limits = Limits {
    capabilities_per_handshake: 16,
    error_message_bytes: 512,
    frames_per_session: 32,
    manifest_sha256_bytes: 64,
    nesting_depth: 4,
    request_id_bytes: 64,
    worker_identity_bytes: 128,
    worker_version_bytes: 64,
};

impl Limits {
    fn values_with_bounds(self) -> [(&'static str, usize, usize); 8] {
        [
            (
                "capabilities_per_handshake",
                self.capabilities_per_handshake,
                PHASE_00_LIMIT_BOUNDS.capabilities_per_handshake,
            ),
            (
                "error_message_bytes",
                self.error_message_bytes,
                PHASE_00_LIMIT_BOUNDS.error_message_bytes,
            ),
            (
                "frames_per_session",
                self.frames_per_session,
                PHASE_00_LIMIT_BOUNDS.frames_per_session,
            ),
            (
                "manifest_sha256_bytes",
                self.manifest_sha256_bytes,
                PHASE_00_LIMIT_BOUNDS.manifest_sha256_bytes,
            ),
            (
                "nesting_depth",
                self.nesting_depth,
                PHASE_00_LIMIT_BOUNDS.nesting_depth,
            ),
            (
                "request_id_bytes",
                self.request_id_bytes,
                PHASE_00_LIMIT_BOUNDS.request_id_bytes,
            ),
            (
                "worker_identity_bytes",
                self.worker_identity_bytes,
                PHASE_00_LIMIT_BOUNDS.worker_identity_bytes,
            ),
            (
                "worker_version_bytes",
                self.worker_version_bytes,
                PHASE_00_LIMIT_BOUNDS.worker_version_bytes,
            ),
        ]
    }
}

// These allowances apply only to generated upstream code emitted by prost-build.
#[allow(clippy::doc_markdown, clippy::trivially_copy_pass_by_ref)]
mod wire {
    include!(concat!(env!("OUT_DIR"), "/eutheto.solver.worker.v1.rs"));
}

use wire::{
    HealthState, SolverWorkerFrame, WorkerCapability, WorkerErrorCode,
    solver_worker_frame as frame, worker_request, worker_result,
};

pub fn verify(repo_root: &Path) -> Result<()> {
    let contract = read_contract(repo_root)?;
    validate_contract(&contract)?;
    let golden_root = repo_root.join("protocol/golden");
    let fixtures = discover_pairs(&golden_root)?;

    let expected_names = BTreeSet::from([
        "handshake-request",
        "handshake-result",
        "health-request",
        "health-result",
        "unsupported-version-error",
        "unsupported-version-request",
    ]);
    let actual_names = fixtures.keys().map(String::as_str).collect::<BTreeSet<_>>();
    ensure!(
        actual_names == expected_names,
        "protocol golden fixture set is incomplete or contains an unsupported pair"
    );

    for (name, pair) in &fixtures {
        verify_pair(name, pair, &contract)?;
    }

    println!("verified {} protocol fixture pair(s)", fixtures.len());
    Ok(())
}

fn read_contract(repo_root: &Path) -> Result<VersionContract> {
    let path = repo_root.join("protocol/version.json");
    let source = fs::read_to_string(&path)
        .with_context(|| format!("failed to read protocol contract {}", path.display()))?;
    serde_json::from_str(&source)
        .with_context(|| format!("invalid protocol contract JSON in {}", path.display()))
}

fn validate_contract(contract: &VersionContract) -> Result<()> {
    ensure!(
        contract.protocol == "eutheto.solver-worker",
        "unexpected protocol namespace: {}",
        contract.protocol
    );
    ensure!(
        contract.wire_version == 1,
        "Phase-00 protocol wire version must remain 1"
    );
    ensure!(
        contract.compatibility.accepted_wire_versions == [contract.wire_version],
        "Phase-00 readers must accept exactly the current wire version"
    );
    ensure!(
        contract.framing.length_prefix_bytes == 4,
        "only the specified four-byte frame prefix is supported"
    );
    ensure!(
        contract.framing.length_prefix_order == "big-endian",
        "only the specified big-endian frame prefix is supported"
    );
    ensure!(
        contract.framing.min_payload_bytes > 0
            && contract.framing.min_payload_bytes <= contract.framing.max_payload_bytes
            && contract.framing.max_payload_bytes <= PHASE_00_MAX_PAYLOAD_BYTES,
        "invalid protocol payload bounds: expected 0 < min <= max <= {PHASE_00_MAX_PAYLOAD_BYTES}"
    );
    for (name, value, maximum) in contract.limits.values_with_bounds() {
        ensure!(value > 0, "protocol limit {name} must be positive");
        ensure!(
            value <= maximum,
            "protocol limit {name} exceeds its Phase-00 maximum of {maximum}: {value}"
        );
    }
    ensure!(
        contract.limits.manifest_sha256_bytes == PHASE_00_LIMIT_BOUNDS.manifest_sha256_bytes,
        "protocol limit manifest_sha256_bytes must remain exactly {}",
        PHASE_00_LIMIT_BOUNDS.manifest_sha256_bytes
    );
    ensure!(
        contract.compatibility.unknown_version_action == "bounded_error_then_close",
        "unknown-version behavior is not the bounded Phase-00 contract"
    );
    Ok(())
}

#[derive(Debug)]
struct FixturePair {
    json: PathBuf,
    frame: PathBuf,
}

#[derive(Default)]
struct PartialPair {
    json: Option<PathBuf>,
    frame: Option<PathBuf>,
}

fn discover_pairs(root: &Path) -> Result<BTreeMap<String, FixturePair>> {
    let mut partial = BTreeMap::<String, PartialPair>::new();
    let mut paths = fs::read_dir(root)
        .with_context(|| format!("failed to read protocol fixtures {}", root.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("failed to inspect protocol fixtures {}", root.display()))?;
    paths.sort_by_key(fs::DirEntry::file_name);

    for entry in paths {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect protocol fixture {}", path.display()))?;
        ensure!(
            file_type.is_file() && !file_type.is_symlink(),
            "protocol golden entries must be regular files: {}",
            path.display()
        );
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow!("protocol fixture filename is not valid UTF-8"))?;

        if let Some(stem) = name.strip_suffix(".frame.hex") {
            ensure!(!stem.is_empty(), "protocol fixture has an empty name");
            let slot = partial.entry(stem.to_owned()).or_default();
            ensure!(
                slot.frame.replace(path).is_none(),
                "duplicate frame fixture {stem}"
            );
        } else if let Some(stem) = name.strip_suffix(".json") {
            ensure!(!stem.is_empty(), "protocol fixture has an empty name");
            let slot = partial.entry(stem.to_owned()).or_default();
            ensure!(
                slot.json.replace(path).is_none(),
                "duplicate JSON fixture {stem}"
            );
        } else {
            bail!("unsupported file in protocol/golden: {name}");
        }
    }

    partial
        .into_iter()
        .map(|(name, pair)| {
            let json = pair
                .json
                .with_context(|| format!("protocol fixture {name} is missing its JSON file"))?;
            let frame = pair
                .frame
                .with_context(|| format!("protocol fixture {name} is missing its frame file"))?;
            Ok((name, FixturePair { json, frame }))
        })
        .collect()
}

fn verify_pair(name: &str, pair: &FixturePair, contract: &VersionContract) -> Result<()> {
    let expected_source = fs::read_to_string(&pair.json)
        .with_context(|| format!("failed to read {}", pair.json.display()))?;
    let expected = parse_canonical_json_fixture(&expected_source)
        .with_context(|| format!("invalid canonical JSON fixture {}", pair.json.display()))?;
    let hex_source = fs::read_to_string(&pair.frame)
        .with_context(|| format!("failed to read {}", pair.frame.display()))?;
    let framed = parse_canonical_hex_fixture(&hex_source)
        .with_context(|| format!("invalid canonical hex fixture {}", pair.frame.display()))?;

    ensure!(
        framed.len() >= 4,
        "protocol fixture {name} is shorter than its prefix"
    );
    let declared = usize::try_from(u32::from_be_bytes([
        framed[0], framed[1], framed[2], framed[3],
    ]))
    .context("frame length does not fit this platform")?;
    let payload = &framed[4..];
    ensure!(
        declared == payload.len(),
        "protocol fixture {name} declares {declared} bytes but contains {}",
        payload.len()
    );
    ensure!(
        (contract.framing.min_payload_bytes..=contract.framing.max_payload_bytes)
            .contains(&payload.len()),
        "protocol fixture {name} violates payload bounds"
    );

    let decoded = SolverWorkerFrame::decode(payload)
        .with_context(|| format!("failed to decode protobuf fixture {name}"))?;
    validate_frame(&decoded, contract)
        .with_context(|| format!("protocol fixture {name} violates declared limits"))?;

    let mut canonical_payload = Vec::new();
    decoded
        .encode(&mut canonical_payload)
        .with_context(|| format!("failed to re-encode protocol fixture {name}"))?;
    ensure!(
        canonical_payload == payload,
        "protocol fixture {name} is not canonical or contains unknown fields"
    );

    let actual = frame_json(&decoded)
        .with_context(|| format!("protocol fixture {name} has an unsupported message shape"))?;
    ensure!(
        actual == expected,
        "JSON and frame disagree for protocol fixture {name}"
    );
    Ok(())
}

fn parse_canonical_json_fixture(source: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(source).context("invalid JSON")?;
    let canonical = format!("{}\n", serde_json::to_string_pretty(&value)?);
    ensure!(
        source == canonical,
        "JSON must use sorted keys, two-space indentation, and one trailing LF"
    );
    Ok(value)
}

fn parse_canonical_hex_fixture(source: &str) -> Result<Vec<u8>> {
    let digits = source
        .strip_suffix('\n')
        .context("hex fixture must end with one LF")?;
    ensure!(!digits.is_empty(), "hex fixture is empty");
    ensure!(
        !digits.ends_with('\n')
            && digits
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "hex fixture must contain lowercase digits only and one trailing LF"
    );
    decode_hex(digits)
}

fn validate_frame(frame: &SolverWorkerFrame, contract: &VersionContract) -> Result<()> {
    let body = frame.body.as_ref().context("frame body is absent")?;
    let version_is_accepted = contract
        .compatibility
        .accepted_wire_versions
        .contains(&frame.protocol_version);

    if !version_is_accepted {
        validate_request_id(&frame.request_id, true, contract)?;
        let frame::Body::Request(request) = body else {
            bail!("only a request may carry an unsupported protocol version");
        };
        request.kind.as_ref().context("request kind is absent")?;
        return Ok(());
    }

    ensure!(
        frame.protocol_version == contract.wire_version,
        "accepted frame does not use the current wire version"
    );

    match body {
        frame::Body::Request(request) => {
            validate_request_id(&frame.request_id, true, contract)?;
            request.kind.as_ref().context("request kind is absent")?;
        }
        frame::Body::Result(result) => {
            validate_request_id(&frame.request_id, true, contract)?;
            match result.kind.as_ref().context("result kind is absent")? {
                worker_result::Kind::Handshake(handshake) => {
                    validate_handshake_result(handshake, contract)?;
                }
                worker_result::Kind::Health(health) => {
                    ensure!(
                        HealthState::try_from(health.state)
                            .is_ok_and(|state| state == HealthState::Ready),
                        "health result state is zero or unknown"
                    );
                }
            }
        }
        frame::Body::Error(error) => validate_worker_error(frame, error, contract)?,
    }
    Ok(())
}

fn validate_handshake_result(
    handshake: &wire::HandshakeResult,
    contract: &VersionContract,
) -> Result<()> {
    validate_printable_text("worker identity", &handshake.worker_identity, true)?;
    validate_printable_text("worker version", &handshake.worker_version, true)?;
    ensure!(
        handshake.worker_identity.len() <= contract.limits.worker_identity_bytes,
        "worker identity exceeds its byte limit"
    );
    ensure!(
        handshake.worker_version.len() <= contract.limits.worker_version_bytes,
        "worker version exceeds its byte limit"
    );
    ensure!(
        handshake.manifest_sha256.len() == contract.limits.manifest_sha256_bytes,
        "manifest SHA-256 has the wrong length"
    );
    ensure!(
        handshake
            .manifest_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "manifest SHA-256 is not lowercase hexadecimal"
    );
    ensure!(
        handshake.capabilities.len() <= contract.limits.capabilities_per_handshake,
        "handshake capability count exceeds its limit"
    );
    let unique = handshake.capabilities.iter().collect::<BTreeSet<_>>();
    ensure!(
        unique.len() == handshake.capabilities.len(),
        "handshake capabilities contain duplicates"
    );
    for capability in &handshake.capabilities {
        ensure!(
            WorkerCapability::try_from(*capability)
                .is_ok_and(|value| value != WorkerCapability::Unspecified),
            "handshake capability is zero or unknown"
        );
    }
    Ok(())
}

fn validate_worker_error(
    frame: &SolverWorkerFrame,
    error: &wire::WorkerError,
    contract: &VersionContract,
) -> Result<()> {
    ensure!(
        error.message.len() <= contract.limits.error_message_bytes,
        "worker error message exceeds its byte limit"
    );
    validate_printable_text("worker error message", &error.message, true)?;
    let code = WorkerErrorCode::try_from(error.code).context("unknown worker error code")?;
    ensure!(
        code != WorkerErrorCode::Unspecified,
        "worker error code is unspecified"
    );
    if code == WorkerErrorCode::UnsupportedProtocolVersion {
        ensure!(
            frame.request_id.is_empty(),
            "unsupported-version error must not echo a request ID"
        );
        ensure!(
            error.message == "unsupported protocol version",
            "unsupported-version error has noncanonical text"
        );
        let rejected = error
            .rejected_protocol_version
            .context("unsupported-version error omits the rejected version")?;
        ensure!(
            !contract
                .compatibility
                .accepted_wire_versions
                .contains(&rejected),
            "unsupported-version error rejects an accepted version"
        );
    } else {
        validate_request_id(&frame.request_id, true, contract)?;
        ensure!(
            error.rejected_protocol_version.is_none(),
            "rejected protocol version is present on another error code"
        );
    }
    Ok(())
}

fn validate_request_id(value: &str, required: bool, contract: &VersionContract) -> Result<()> {
    if required {
        ensure!(!value.is_empty(), "request ID is absent");
    } else if value.is_empty() {
        return Ok(());
    }
    ensure!(
        value.len() <= contract.limits.request_id_bytes,
        "request ID exceeds its byte limit"
    );
    ensure!(
        value
            .bytes()
            .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') }),
        "request ID contains a forbidden character"
    );
    Ok(())
}

fn validate_printable_text(name: &str, value: &str, required: bool) -> Result<()> {
    if required {
        ensure!(!value.is_empty(), "{name} is empty");
    }
    ensure!(
        value.chars().all(|character| !character.is_control()),
        "{name} contains a control character"
    );
    Ok(())
}

fn frame_json(frame: &SolverWorkerFrame) -> Result<Value> {
    let mut object = serde_json::Map::new();
    object.insert("protocolVersion".to_owned(), json!(frame.protocol_version));
    if !frame.request_id.is_empty() {
        object.insert("requestId".to_owned(), json!(frame.request_id));
    }

    match frame.body.as_ref().context("frame body is absent")? {
        frame::Body::Request(request) => {
            let request = match request.kind.as_ref().context("request kind is absent")? {
                worker_request::Kind::Handshake(_) => json!({ "handshake": {} }),
                worker_request::Kind::Health(_) => json!({ "health": {} }),
            };
            object.insert("request".to_owned(), request);
        }
        frame::Body::Result(result) => {
            let result = match result.kind.as_ref().context("result kind is absent")? {
                worker_result::Kind::Handshake(handshake) => {
                    let capabilities = handshake
                        .capabilities
                        .iter()
                        .map(|value| capability_name(*value))
                        .collect::<Result<Vec<_>>>()?;
                    json!({
                        "handshake": {
                            "capabilities": capabilities,
                            "manifestSha256": handshake.manifest_sha256,
                            "workerIdentity": handshake.worker_identity,
                            "workerVersion": handshake.worker_version,
                        }
                    })
                }
                worker_result::Kind::Health(health) => {
                    json!({ "health": { "state": health_state_name(health.state)? } })
                }
            };
            object.insert("result".to_owned(), result);
        }
        frame::Body::Error(error) => {
            let mut body = serde_json::Map::new();
            body.insert("code".to_owned(), json!(error_code_name(error.code)?));
            body.insert("message".to_owned(), json!(error.message));
            if let Some(version) = error.rejected_protocol_version {
                body.insert("rejectedProtocolVersion".to_owned(), json!(version));
            }
            object.insert("error".to_owned(), Value::Object(body));
        }
    }

    Ok(Value::Object(object))
}

fn capability_name(value: i32) -> Result<&'static str> {
    match WorkerCapability::try_from(value).context("unknown worker capability")? {
        WorkerCapability::Unspecified => bail!("worker capability is unspecified"),
        WorkerCapability::Health => Ok("WORKER_CAPABILITY_HEALTH"),
    }
}

fn health_state_name(value: i32) -> Result<&'static str> {
    match HealthState::try_from(value).context("unknown health state")? {
        HealthState::Unspecified => bail!("health state is unspecified"),
        HealthState::Ready => Ok("HEALTH_STATE_READY"),
    }
}

fn error_code_name(value: i32) -> Result<&'static str> {
    match WorkerErrorCode::try_from(value).context("unknown worker error code")? {
        WorkerErrorCode::Unspecified => bail!("worker error code is unspecified"),
        WorkerErrorCode::MalformedFrame => Ok("WORKER_ERROR_CODE_MALFORMED_FRAME"),
        WorkerErrorCode::UnsupportedProtocolVersion => {
            Ok("WORKER_ERROR_CODE_UNSUPPORTED_PROTOCOL_VERSION")
        }
        WorkerErrorCode::ResourceLimit => Ok("WORKER_ERROR_CODE_RESOURCE_LIMIT"),
        WorkerErrorCode::UnsupportedRequest => Ok("WORKER_ERROR_CODE_UNSUPPORTED_REQUEST"),
        WorkerErrorCode::Internal => Ok("WORKER_ERROR_CODE_INTERNAL"),
    }
}

fn decode_hex(source: &str) -> Result<Vec<u8>> {
    ensure!(
        source.len().is_multiple_of(2),
        "hex input has an odd number of digits"
    );
    source
        .as_bytes()
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
    use std::path::Path;

    use serde_json::{Map, Value, json};

    use super::{
        VersionContract, decode_hex, parse_canonical_hex_fixture, parse_canonical_json_fixture,
        validate_contract, validate_frame, verify,
        wire::{
            HandshakeRequest, HandshakeResult, HealthResult, HealthState, SolverWorkerFrame,
            WorkerCapability, WorkerError, WorkerErrorCode, WorkerRequest, WorkerResult,
            solver_worker_frame as frame, worker_request, worker_result,
        },
    };

    fn contract_json() -> Result<Value, serde_json::Error> {
        serde_json::from_str(include_str!("../../protocol/version.json"))
    }

    fn limits_object(value: &mut Value) -> anyhow::Result<&mut Map<String, Value>> {
        value
            .get_mut("limits")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| anyhow::anyhow!("checked-in protocol contract has no limits object"))
    }

    #[test]
    fn version_contract_rejects_unknown_fields() -> anyhow::Result<()> {
        for (pointer, location) in [
            ("", "root"),
            ("/compatibility", "compatibility"),
            ("/framing", "framing"),
            ("/limits", "limits"),
        ] {
            let mut value = contract_json()?;
            let object = value
                .pointer_mut(pointer)
                .and_then(Value::as_object_mut)
                .ok_or_else(|| anyhow::anyhow!("contract location {location} is not an object"))?;
            object.insert("unexpected".to_owned(), json!(true));

            assert!(
                serde_json::from_value::<VersionContract>(value).is_err(),
                "version contract accepted an unknown field in {location}"
            );
        }
        Ok(())
    }

    #[test]
    fn version_contract_requires_every_limit() -> anyhow::Result<()> {
        let contract: VersionContract = serde_json::from_value(contract_json()?)?;

        for (name, _, _) in contract.limits.values_with_bounds() {
            let mut value = contract_json()?;
            let removed = limits_object(&mut value)?.remove(name);
            assert!(removed.is_some(), "checked-in contract has no {name} limit");
            assert!(
                serde_json::from_value::<VersionContract>(value).is_err(),
                "version contract accepted a missing {name} limit"
            );
        }
        Ok(())
    }

    #[test]
    fn version_contract_rejects_zero_and_over_bound_limits() -> anyhow::Result<()> {
        let contract: VersionContract = serde_json::from_value(contract_json()?)?;

        for (name, _, maximum) in contract.limits.values_with_bounds() {
            for invalid in [0, maximum + 1] {
                let mut value = contract_json()?;
                limits_object(&mut value)?.insert(name.to_owned(), json!(invalid));
                let invalid_contract: VersionContract = serde_json::from_value(value)?;
                assert!(
                    validate_contract(&invalid_contract).is_err(),
                    "version contract accepted {name}={invalid}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn frame_validation_rejects_semantically_invalid_values() -> anyhow::Result<()> {
        let contract: VersionContract = serde_json::from_value(contract_json()?)?;
        let request = WorkerRequest {
            kind: Some(worker_request::Kind::Handshake(HandshakeRequest {})),
        };
        let mut frame = SolverWorkerFrame {
            protocol_version: contract.wire_version,
            request_id: "phase00-1".to_owned(),
            body: Some(frame::Body::Request(request)),
        };

        validate_frame(&frame, &contract)?;
        frame.request_id.clear();
        assert!(validate_frame(&frame, &contract).is_err());
        frame.request_id = "bad/request".to_owned();
        assert!(validate_frame(&frame, &contract).is_err());

        frame.protocol_version = contract.wire_version;
        frame.request_id = "phase00-health".to_owned();
        frame.body = Some(frame::Body::Result(WorkerResult {
            kind: Some(worker_result::Kind::Health(HealthResult {
                state: HealthState::Unspecified as i32,
            })),
        }));
        assert!(validate_frame(&frame, &contract).is_err());

        frame.body = Some(frame::Body::Result(WorkerResult {
            kind: Some(worker_result::Kind::Handshake(HandshakeResult {
                worker_identity: "eutheto-ortools-worker".to_owned(),
                worker_version: "fixture".to_owned(),
                manifest_sha256: "0".repeat(64),
                capabilities: vec![WorkerCapability::Unspecified as i32],
            })),
        }));
        assert!(validate_frame(&frame, &contract).is_err());

        frame.request_id = "phase00-error".to_owned();
        frame.body = Some(frame::Body::Error(WorkerError {
            code: WorkerErrorCode::MalformedFrame as i32,
            message: "malformed frame".to_owned(),
            rejected_protocol_version: Some(2),
        }));
        assert!(validate_frame(&frame, &contract).is_err());

        frame.request_id.clear();
        frame.body = Some(frame::Body::Error(WorkerError {
            code: WorkerErrorCode::UnsupportedProtocolVersion as i32,
            message: "unsupported protocol version".to_owned(),
            rejected_protocol_version: Some(2),
        }));
        validate_frame(&frame, &contract)?;
        Ok(())
    }

    #[test]
    fn fixture_text_validation_rejects_noncanonical_forms() {
        assert!(parse_canonical_hex_fixture("00ff\n").is_ok());
        assert!(parse_canonical_hex_fixture("00FF\n").is_err());
        assert!(parse_canonical_hex_fixture("00ff").is_err());
        assert!(parse_canonical_hex_fixture("00ff\n\n").is_err());

        assert!(parse_canonical_json_fixture("{\n  \"a\": 1,\n  \"b\": 2\n}\n").is_ok());
        assert!(parse_canonical_json_fixture("{\"a\":1,\"b\":2}\n").is_err());
        assert!(parse_canonical_json_fixture("{\n  \"b\": 2,\n  \"a\": 1\n}\n").is_err());
        assert!(parse_canonical_json_fixture("{\n  \"a\": 1,\n  \"b\": 2\n}").is_err());
    }

    #[test]
    fn hex_decoder_rejects_truncated_and_invalid_input() -> anyhow::Result<()> {
        assert!(decode_hex("0").is_err());
        assert!(decode_hex("0g").is_err());
        assert_eq!(decode_hex("00ff")?, vec![0, 255]);
        Ok(())
    }

    #[test]
    fn checked_in_protocol_fixtures_match_frames() -> anyhow::Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| anyhow::anyhow!("xtask manifest has no repository parent"))?;
        verify(root)
    }
}
