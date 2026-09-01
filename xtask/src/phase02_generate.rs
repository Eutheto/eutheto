use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::fmt::Write as _;

const PACK_SOURCE: &str = include_str!("../../schemas/domain-packs/official-test.contract.json");
const MATRIX_SOURCE: &str = include_str!("../../schemas/solver-support-matrix.json");

const GENERATED_RUST_PACK: &str =
    "crates/eutheto-command/src/generated_official_test_pack_contract.rs";
const GENERATED_TYPESCRIPT_PACK: &str = "apps/desktop/src/api/generated-domain-pack-contracts.ts";
const GENERATED_COMMAND_SCHEMAS: &str = "schemas/generated/official-test.command-schemas.json";
const GENERATED_PORTABLE_SCHEMA: &str = "schemas/generated/official-test.portable.schema.json";
const GENERATED_SHARE_SCHEMA: &str = "schemas/generated/official-test.share-result.schema.json";
const GENERATED_AI_TOOLS: &str = "xtask/generated/official-test-ai-tools.json";
const GENERATED_UI_MANIFEST: &str = "xtask/generated/official-test-ui-manifest.json";
const GENERATED_PACK_DOCS: &str = "docs/generated/official-test-pack-contract.md";
const GENERATED_RUST_MATRIX: &str = "crates/eutheto-solver-api/src/generated_support_matrix.rs";
const GENERATED_MATRIX_DOCS: &str = "docs/generated/solver-support-matrix.md";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackContract {
    schema_version: u32,
    pack: PackDescriptor,
    commands: Vec<CommandContract>,
    portable_schema: Value,
    share_result_schema: Value,
    ai_tools: Vec<Value>,
    ui_manifest: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackDescriptor {
    id: String,
    pack_version: String,
    latest_schema_version: u32,
    portable_schema_version: u32,
    share_result_schema_version: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandContract {
    id: String,
    title: LocalizedText,
    description: LocalizedText,
    risk: String,
    reversibility: String,
    ai_grouping_allowed: bool,
    payload_schema: Value,
    result_schema: Value,
    change_schema: Value,
    valid_examples: Vec<Value>,
    invalid_examples: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalizedText {
    key: String,
    default_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SupportMatrix {
    schema_version: u32,
    planning_ir_schema_version: u32,
    features: Vec<SupportFeature>,
    registered_backends: Vec<RegisteredBackend>,
    deferred_candidate_gates: Vec<DeferredCandidateGate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SupportFeature {
    id: String,
    category: String,
    gate: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegisteredBackend {
    id: String,
    version: String,
    adapter_version: String,
    support: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeferredCandidateGate {
    backend_id: String,
    candidate_version: String,
    claim_status: String,
    owning_phase: u32,
}

pub fn generated_files() -> Result<Vec<(&'static str, String)>> {
    let pack = parse_pack()?;
    let matrix = parse_matrix()?;
    validate_pack(&pack)?;
    validate_matrix(&matrix)?;

    Ok(vec![
        (GENERATED_RUST_PACK, render_rust_pack(&pack)?),
        (GENERATED_TYPESCRIPT_PACK, render_typescript_pack(&pack)?),
        (GENERATED_COMMAND_SCHEMAS, render_command_schemas(&pack)?),
        (
            GENERATED_PORTABLE_SCHEMA,
            render_schema(&pack.portable_schema, "official.test portable schema")?,
        ),
        (
            GENERATED_SHARE_SCHEMA,
            render_schema(
                &pack.share_result_schema,
                "official.test Share Result schema",
            )?,
        ),
        (GENERATED_AI_TOOLS, render_ai_tools(&pack)?),
        (GENERATED_UI_MANIFEST, render_ui_manifest(&pack)?),
        (GENERATED_PACK_DOCS, render_pack_docs(&pack)),
        (GENERATED_RUST_MATRIX, render_rust_matrix(&matrix)),
        (GENERATED_MATRIX_DOCS, render_matrix_docs(&matrix)),
    ])
}

fn parse_pack() -> Result<PackContract> {
    serde_json::from_str(PACK_SOURCE).context("invalid official.test contract source")
}

fn parse_matrix() -> Result<SupportMatrix> {
    serde_json::from_str(MATRIX_SOURCE).context("invalid solver support matrix source")
}

fn validate_pack(pack: &PackContract) -> Result<()> {
    if pack.schema_version != 1 {
        bail!("unsupported official.test contract source version")
    }
    if pack.pack.id != "official.test"
        || pack.pack.latest_schema_version == 0
        || pack.pack.portable_schema_version == 0
        || pack.pack.share_result_schema_version == 0
    {
        bail!("official.test descriptor versions or identity are invalid")
    }
    semver::Version::parse(&pack.pack.pack_version)
        .context("official.test packVersion is not semantic versioning")?;

    let mut prior_id: Option<&str> = None;
    for command in &pack.commands {
        if !command.id.starts_with("official.test.") {
            bail!(
                "command {} is outside the official.test namespace",
                command.id
            )
        }
        if prior_id.is_some_and(|prior| prior >= command.id.as_str()) {
            bail!("official.test commands must be uniquely sorted by id")
        }
        prior_id = Some(&command.id);
        for schema in [
            &command.payload_schema,
            &command.result_schema,
            &command.change_schema,
        ] {
            require_strict_object_schema(schema, &command.id)?;
        }
        if command.valid_examples.is_empty() || command.invalid_examples.is_empty() {
            bail!("command {} requires valid and invalid examples", command.id)
        }
        if command.title.key.is_empty()
            || command.title.default_text.is_empty()
            || command.description.key.is_empty()
            || command.description.default_text.is_empty()
            || command.risk.is_empty()
            || command.reversibility.is_empty()
        {
            bail!("command {} has incomplete metadata", command.id)
        }
    }
    let command_ids = pack
        .commands
        .iter()
        .map(|command| command.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let ai_command_ids = pack
        .ai_tools
        .iter()
        .map(|tool| {
            tool.get("commandId")
                .and_then(Value::as_str)
                .context("AI tool commandId must be a string")
        })
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    if pack.ai_tools.len() != command_ids.len() || ai_command_ids != command_ids {
        bail!("AI tools must cover every command exactly once")
    }
    let manifest = pack
        .ui_manifest
        .as_object()
        .context("uiManifest must be an object")?;
    for required in [
        "setupSteps",
        "entityKinds",
        "ruleKinds",
        "resultViews",
        "importers",
        "exporters",
    ] {
        if manifest
            .get(required)
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
        {
            bail!("uiManifest.{required} must be a nonempty array")
        }
    }
    require_strict_object_schema(&pack.portable_schema, "portableSchema")?;
    require_strict_object_schema(&pack.share_result_schema, "shareResultSchema")?;
    Ok(())
}

fn require_strict_object_schema(schema: &Value, owner: &str) -> Result<()> {
    let object = schema
        .as_object()
        .with_context(|| format!("{owner} schema must be an object"))?;
    if object.get("type").and_then(Value::as_str) != Some("object")
        || object.get("additionalProperties").and_then(Value::as_bool) != Some(false)
    {
        bail!("{owner} schema must be a strict object")
    }
    Ok(())
}

fn validate_matrix(matrix: &SupportMatrix) -> Result<()> {
    if matrix.schema_version != 1 || matrix.planning_ir_schema_version != 1 {
        bail!("unsupported support-matrix or planning-IR schema version")
    }
    let mut prior_feature: Option<&str> = None;
    for feature in &matrix.features {
        if prior_feature.is_some_and(|prior| prior >= feature.id.as_str()) {
            bail!("support-matrix features must be uniquely sorted by id")
        }
        if feature.gate != "unconditional" {
            bail!("Phase-02 matrix may contain only enabled unconditional features")
        }
        prior_feature = Some(&feature.id);
    }
    let feature_ids = matrix
        .features
        .iter()
        .map(|feature| feature.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for backend in &matrix.registered_backends {
        if backend.id.is_empty() || backend.version.is_empty() || backend.adapter_version.is_empty()
        {
            bail!("registered backend descriptors must be complete")
        }
        if backend
            .support
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>()
            != feature_ids
        {
            bail!(
                "backend {} must declare every feature exactly once",
                backend.id
            )
        }
    }
    for candidate in &matrix.deferred_candidate_gates {
        if candidate.claim_status != "unclaimed" {
            bail!(
                "deferred candidate {} must remain unclaimed",
                candidate.backend_id
            )
        }
    }
    Ok(())
}

fn render_rust_pack(pack: &PackContract) -> Result<String> {
    let canonical = pretty_json(&serde_json::from_str::<Value>(PACK_SOURCE)?)?;
    let source_hash = blake3::hash(PACK_SOURCE.as_bytes()).to_hex();
    let command_ids = pack
        .commands
        .iter()
        .map(|command| format!("{:?}", command.id))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "// SPDX-License-Identifier: Apache-2.0\n// @generated by `cargo xtask generate` from schemas/domain-packs/official-test.contract.json; do not edit.\n// source-blake3: {source_hash}\n\npub const OFFICIAL_TEST_PACK_ID: &str = {:?};\npub const OFFICIAL_TEST_PACK_VERSION: &str = {:?};\npub const OFFICIAL_TEST_PACK_CONTRACT_JSON: &str = r#\"{}\"#;\npub const OFFICIAL_TEST_COMMAND_IDS: &[&str] = &[{command_ids}];\n",
        pack.pack.id, pack.pack.pack_version, canonical
    ))
}

fn render_typescript_pack(pack: &PackContract) -> Result<String> {
    let canonical = pretty_json(&serde_json::from_str::<Value>(PACK_SOURCE)?)?;
    let source_hash = blake3::hash(PACK_SOURCE.as_bytes()).to_hex();
    let command = pack
        .commands
        .first()
        .context("official.test requires one generated command")?;
    Ok(format!(
        "// SPDX-License-Identifier: Apache-2.0\n// @generated by `cargo xtask generate` from schemas/domain-packs/official-test.contract.json; do not edit.\n// source-blake3: {source_hash}\n\nexport interface OfficialTestConfigureEntityPayload {{\n  readonly entityId: string;\n  readonly enabled: boolean;\n  readonly target: number;\n}}\n\nexport const OFFICIAL_TEST_PACK_ID = {:?} as const;\nexport const OFFICIAL_TEST_CONFIGURE_ENTITY_COMMAND_ID = {:?} as const;\nexport const OFFICIAL_TEST_PACK_CONTRACT_JSON = String.raw`{}`;\n",
        pack.pack.id, command.id, canonical
    ))
}

fn render_command_schemas(pack: &PackContract) -> Result<String> {
    let mut commands = Map::new();
    for command in &pack.commands {
        commands.insert(
            command.id.clone(),
            json!({
                "payload": command.payload_schema,
                "result": command.result_schema,
                "change": command.change_schema,
                "validExamples": command.valid_examples,
                "invalidExamples": command.invalid_examples,
                "risk": command.risk,
                "reversibility": command.reversibility,
                "aiGroupingAllowed": command.ai_grouping_allowed,
            }),
        );
    }
    pretty_json(&json!({
        "schemaVersion": pack.schema_version,
        "packId": pack.pack.id,
        "commands": commands,
    }))
}

fn render_ai_tools(pack: &PackContract) -> Result<String> {
    pretty_json(&json!({
        "schemaVersion": pack.schema_version,
        "$comment": "Generated by cargo xtask generate from schemas/domain-packs/official-test.contract.json; do not edit.",
        "packId": pack.pack.id,
        "tools": pack.ai_tools,
    }))
}

fn render_ui_manifest(pack: &PackContract) -> Result<String> {
    pretty_json(&json!({
        "schemaVersion": pack.schema_version,
        "$comment": "Generated by cargo xtask generate from schemas/domain-packs/official-test.contract.json; do not edit.",
        "packId": pack.pack.id,
        "manifest": pack.ui_manifest,
    }))
}

fn render_schema(schema: &Value, owner: &str) -> Result<String> {
    let mut schema = schema.clone();
    let object = schema
        .as_object_mut()
        .with_context(|| format!("{owner} must be an object"))?;
    object.insert(
        "$comment".to_owned(),
        Value::String(format!(
            "Generated by cargo xtask generate from schemas/domain-packs/official-test.contract.json ({owner}); do not edit."
        )),
    );
    pretty_json(&schema)
}

fn pretty_json(value: &impl serde::Serialize) -> Result<String> {
    let mut output =
        serde_json::to_string_pretty(value).context("failed to render generated JSON")?;
    output.push('\n');
    Ok(output)
}

fn render_pack_docs(pack: &PackContract) -> String {
    let source_hash = blake3::hash(PACK_SOURCE.as_bytes()).to_hex();
    let mut output = format!(
        "<!-- SPDX-License-Identifier: Apache-2.0 -->\n<!-- @generated by `cargo xtask generate` from `schemas/domain-packs/official-test.contract.json`; do not edit. -->\n<!-- source-blake3: {source_hash} -->\n\n# `official.test` generated contract\n\nPack version `{}`, domain schema `{}`, portable schema `{}`, Share Result schema `{}`.\n\n| Command | Risk | Reversibility | AI grouping |\n|---|---|---|---|\n",
        pack.pack.pack_version,
        pack.pack.latest_schema_version,
        pack.pack.portable_schema_version,
        pack.pack.share_result_schema_version,
    );
    for command in &pack.commands {
        let _ = writeln!(
            output,
            "| `{}` | `{}` | `{}` | `{}` |",
            command.id, command.risk, command.reversibility, command.ai_grouping_allowed
        );
    }
    output
}

fn render_rust_matrix(matrix: &SupportMatrix) -> String {
    let source_hash = blake3::hash(MATRIX_SOURCE.as_bytes()).to_hex();
    let mut output = format!(
        "// SPDX-License-Identifier: Apache-2.0\n// @generated by `cargo xtask generate` from schemas/solver-support-matrix.json; do not edit.\n// source-blake3: {source_hash}\n\npub const SUPPORT_MATRIX_SCHEMA_VERSION: u32 = {};\npub const SUPPORT_MATRIX_IR_SCHEMA_VERSION: u32 = {};\npub const SUPPORT_FEATURES: &[(&str, &str, &str)] = &[\n",
        matrix.schema_version, matrix.planning_ir_schema_version
    );
    for feature in &matrix.features {
        let _ = writeln!(
            output,
            "    ({:?}, {:?}, {:?}),",
            feature.id, feature.category, feature.gate
        );
    }
    output.push_str("];\n");
    if matrix.registered_backends.is_empty() {
        output.push_str("pub const PRODUCTION_BACKENDS: &[(&str, &str, &str)] = &[];\n");
    } else {
        output.push_str("pub const PRODUCTION_BACKENDS: &[(&str, &str, &str)] = &[\n");
        for backend in &matrix.registered_backends {
            let _ = writeln!(
                output,
                "    ({:?}, {:?}, {:?}),",
                backend.id, backend.version, backend.adapter_version
            );
        }
        output.push_str("];\n");
    }
    output.push_str("pub const DEFERRED_BACKEND_CANDIDATES: &[(&str, &str, u32)] = &[\n");
    for candidate in &matrix.deferred_candidate_gates {
        let _ = writeln!(
            output,
            "    ({:?}, {:?}, {}),",
            candidate.backend_id, candidate.candidate_version, candidate.owning_phase
        );
    }
    output.push_str("];\n");
    output
}

fn render_matrix_docs(matrix: &SupportMatrix) -> String {
    let source_hash = blake3::hash(MATRIX_SOURCE.as_bytes()).to_hex();
    let mut output = format!(
        "<!-- SPDX-License-Identifier: Apache-2.0 -->\n<!-- @generated by `cargo xtask generate` from `schemas/solver-support-matrix.json`; do not edit. -->\n<!-- source-blake3: {source_hash} -->\n\n# Solver support matrix\n\nPhase 02 registers no production solver backend. Fake exact and deliberately unsupported backends exist only in test fixtures and are not production matrix columns.\n\n| Feature | Category | Gate |\n|---|---|---|\n"
    );
    for feature in &matrix.features {
        let _ = writeln!(
            output,
            "| `{}` | `{}` | `{}` |",
            feature.id, feature.category, feature.gate
        );
    }
    output.push_str("\n## Deferred candidates\n\n| Backend | Candidate version | Claim | Owning phase |\n|---|---|---|---|\n");
    for candidate in &matrix.deferred_candidate_gates {
        let _ = writeln!(
            output,
            "| `{}` | `{}` | `{}` | `{}` |",
            candidate.backend_id,
            candidate.candidate_version,
            candidate.claim_status,
            candidate.owning_phase
        );
    }
    output
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::{generated_files, parse_matrix, parse_pack, validate_matrix, validate_pack};

    #[test]
    fn phase02_sources_are_strict_and_complete() -> Result<()> {
        let pack = parse_pack()?;
        let matrix = parse_matrix()?;
        validate_pack(&pack)?;
        validate_matrix(&matrix)?;
        assert_eq!(generated_files()?.len(), 10);
        assert!(matrix.registered_backends.is_empty());
        assert!(
            matrix
                .features
                .iter()
                .all(|feature| feature.id != "ir.circuit-path")
        );
        let files = generated_files()?;
        let paths = files
            .iter()
            .map(|(path, _)| *path)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(paths.len(), files.len());
        for (_, contents) in files {
            assert!(!contents.contains("phase02.fake"));
            assert!(contents.ends_with('\n'));
        }
        Ok(())
    }

    #[test]
    fn duplicate_ai_tool_coverage_is_rejected() -> Result<()> {
        let mut pack = parse_pack()?;
        pack.ai_tools.push(pack.ai_tools[0].clone());
        assert!(validate_pack(&pack).is_err());
        Ok(())
    }
}
