use crate::generated_official_test_pack_contract::{
    OFFICIAL_TEST_COMMAND_IDS, OFFICIAL_TEST_PACK_CONTRACT_JSON, OFFICIAL_TEST_PACK_ID,
    OFFICIAL_TEST_PACK_VERSION,
};
use eutheto_domain_api::{
    AiToolDescriptor, CommandDescriptor, CompileContext, ContractJsonLimits,
    DOMAIN_BATCH_SCHEMA_VERSION, DomainBatchCommand, DomainCapability, DomainCatalog, DomainChange,
    DomainExplanation, DomainMutation, DomainPack, DomainPackDescriptor, DomainPackError,
    DomainShareResult, DomainUiManifest, DomainValidationReport, HistoricalPortableDomainDocument,
    KindDescriptor, LicenseMetadata, LocalizedText, PortableDomainDocument, PortableImportContext,
    ScenarioVersionDescriptor, ScoreDescriptor, ShareResultOptions, TransferDescriptor,
    validate_contract_value,
};
use eutheto_domain_ir::{
    AcceptedResult, AssignmentValue, DomainAssignmentId, DomainEntityId, DomainEntityKindId,
    DomainEntityRef, NormalizedSolution, OptimizationDirection, RequiredRuleBinding,
    RuleEvaluation, ScoreCategoryId, ScoreLevelId, ScoreLevelValue, ScoreVector,
    VerificationContextV1, VerificationFactId, VerificationReport, VerificationScope,
    VerificationValue, blake3_hex,
};
use eutheto_planning_ir::{
    BoolVariable, BoolVariableId, CandidateValues, Capability, ComparisonOp, CompilerId,
    Constraint, ConstraintRecord, ConstraintTag, InclusiveRange, IntDomain, IntVariable,
    IntVariableId, LinearComparison, LinearExpression, LinearTerm, MetadataKey, ObjectiveLevel,
    ObjectiveLevelId, ObjectivePlan, ObjectiveTerm, ObjectiveTermId, ObjectiveTermKind,
    PLANNING_IR_SCHEMA_VERSION, PROJECTION_SCHEMA_VERSION, PlanningConstraintId, PlanningMetadata,
    PlanningProblem, ProjectionExpression, ProjectionId, ProvenanceId, ProvenanceParameter,
    ProvenanceRecord, ProvenanceSourceKind, SolutionProjection, Variable, project_candidate,
    validate,
};
use eutheto_types::{
    DomainCommandEnvelope, DomainPackRef, PackId, PersonId, RuleId, ScenarioDocument,
    ScenarioDomain, SolutionId, ValidationIssue, ValidationSeverity,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

const CONFIGURE_ENTITY: &str = "official.test.configure_entity";
const PORTABLE_CAPABILITY: &str = "official.test.portable-v1";
const ENTITY_KIND: &str = "official.test.entity";
const SCORE_LEVEL: &str = "official.test.score.target";
const SCORE_CATEGORY: &str = "official.test.score.target-total";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedContract {
    schema_version: u32,
    pack: GeneratedPack,
    commands: Vec<GeneratedCommand>,
    portable_schema: Value,
    share_result_schema: Value,
    ai_tools: Vec<GeneratedAiTool>,
    ui_manifest: GeneratedUiManifest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedPack {
    id: String,
    pack_version: String,
    latest_schema_version: u32,
    portable_schema_version: u32,
    share_result_schema_version: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedCommand {
    id: String,
    title: LocalizedText,
    description: LocalizedText,
    risk: eutheto_domain_api::CommandRisk,
    reversibility: eutheto_domain_api::CommandReversibility,
    ai_grouping_allowed: bool,
    payload_schema: Value,
    result_schema: Value,
    change_schema: Value,
    valid_examples: Vec<Value>,
    invalid_examples: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedAiTool {
    command_id: String,
    name: String,
    description: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedUiManifest {
    setup_steps: Vec<GeneratedUiItem>,
    entity_kinds: Vec<GeneratedUiItem>,
    rule_kinds: Vec<GeneratedUiItem>,
    result_views: Vec<GeneratedUiItem>,
    importers: Vec<GeneratedUiItem>,
    exporters: Vec<GeneratedUiItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedUiItem {
    id: String,
    title_key: String,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TestEntity {
    id: PersonId,
    enabled: bool,
    target: i64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigureEntity {
    entity_id: PersonId,
    enabled: bool,
    target: i64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortableEntity {
    enabled: bool,
    target: i64,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortablePayload {
    schema_version: u32,
    entities: BTreeMap<String, PortableEntity>,
    extensions: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoricalPortablePayloadV0 {
    schema_version: u32,
    entities: BTreeMap<String, PortableEntity>,
    #[serde(default)]
    extensions: BTreeMap<String, Value>,
}

impl DomainPack for crate::OfficialTestPack {
    fn descriptor(&self) -> Result<DomainPackDescriptor, DomainPackError> {
        Ok(DomainPackDescriptor {
            id: PackId::new(OFFICIAL_TEST_PACK_ID).map_err(contract)?,
            display_name: text("official.test.name", "Official synthetic test pack"),
            description: text(
                "official.test.description",
                "Synthetic Phase-02 domain-pack conformance fixture.",
            ),
            pack_version: Version::parse(OFFICIAL_TEST_PACK_VERSION).map_err(contract)?,
            scenario_versions: ScenarioVersionDescriptor {
                latest: 1,
                migratable_from: BTreeSet::new(),
            },
            icon_id: "official.test.icon".to_owned(),
            capabilities: [
                DomainCapability::Commands,
                DomainCapability::Compilation,
                DomainCapability::Projection,
                DomainCapability::Verification,
                DomainCapability::Scoring,
                DomainCapability::PortableData,
                DomainCapability::ShareResult,
                DomainCapability::UiManifest,
                DomainCapability::AiTools,
            ]
            .into_iter()
            .collect(),
            portable_schema_version: 1,
            portable_capabilities: [PORTABLE_CAPABILITY.to_owned()].into_iter().collect(),
            share_result_schema_version: 1,
            documentation_url: None,
            license: LicenseMetadata {
                spdx_expression: "Apache-2.0".to_owned(),
                attribution: "Eutheto contributors".to_owned(),
            },
            synthetic_test_only: true,
        })
    }

    fn catalog(&self) -> Result<DomainCatalog, DomainPackError> {
        generated_catalog()
    }

    fn new_document(
        &self,
        mut shell: ScenarioDocument,
    ) -> Result<ScenarioDocument, DomainPackError> {
        require_pack(&shell)?;
        shell.domain = ScenarioDomain::default();
        shell.domain_pack.schema_version = 1;
        Ok(shell)
    }

    fn migrate_document(
        &self,
        document: ScenarioDocument,
    ) -> Result<ScenarioDocument, DomainPackError> {
        require_pack(&document)?;
        parse_entities(&document)?;
        Ok(document)
    }

    fn validate_fast(&self, document: &ScenarioDocument) -> DomainValidationReport {
        validation_report(document)
    }

    fn validate_full(&self, document: &ScenarioDocument) -> DomainValidationReport {
        validation_report(document)
    }

    fn apply_batch(
        &self,
        document: &ScenarioDocument,
        batch: &DomainBatchCommand,
    ) -> Result<DomainMutation, DomainPackError> {
        require_pack(document)?;
        validate_batch(document, batch)?;
        let catalog = generated_catalog()?;
        let descriptor = catalog
            .command(CONFIGURE_ENTITY)
            .ok_or_else(|| DomainPackError::UnknownCommand(CONFIGURE_ENTITY.to_owned()))?;
        let mut working = document.clone();
        let mut results = Vec::with_capacity(batch.commands.len());
        let mut changes = Vec::with_capacity(batch.commands.len());
        let mut inverses = Vec::with_capacity(batch.commands.len());
        for envelope in &batch.commands {
            validate_contract_value(
                &descriptor.payload_schema,
                &envelope.payload,
                ContractJsonLimits::DEFAULT,
            )?;
            let command: ConfigureEntity = serde_json::from_value(envelope.payload.clone())
                .map_err(|error| payload("/commands/payload", error))?;
            check_target(command.target)?;
            if command.enabled != (command.target >= 1) {
                return Err(DomainPackError::InvalidPayload {
                    path: "/commands/payload".to_owned(),
                    message: "enabled must equal target >= 1".to_owned(),
                });
            }
            let previous_value = working
                .domain
                .entities
                .get(&command.entity_id)
                .cloned()
                .ok_or_else(|| DomainPackError::InvalidPayload {
                    path: format!("/domain/entities/{}", command.entity_id),
                    message: "configured entity does not exist".to_owned(),
                })?;
            let previous: TestEntity = serde_json::from_value(previous_value.clone())
                .map_err(|error| payload("/domain/entities", error))?;
            if previous.id != command.entity_id {
                return Err(DomainPackError::Contract(
                    "entity map key/id mismatch".to_owned(),
                ));
            }
            let next = TestEntity {
                id: command.entity_id,
                enabled: command.enabled,
                target: command.target,
            };
            let next_value =
                serde_json::to_value(&next).map_err(|error| payload("/domain/entities", error))?;
            working
                .domain
                .entities
                .insert(command.entity_id, next_value.clone());
            let result = json!({ "entityId": command.entity_id });
            validate_contract_value(
                &descriptor.result_schema,
                &result,
                ContractJsonLimits::DEFAULT,
            )?;
            let change = json!({
                "path": format!("/domain/entities/{}", command.entity_id),
                "before": previous_value,
                "after": next_value,
            });
            validate_contract_value(
                &descriptor.change_schema,
                &change,
                ContractJsonLimits::DEFAULT,
            )?;
            results.push(result);
            changes.push(DomainChange {
                command_id: CONFIGURE_ENTITY.to_owned(),
                value: change,
            });
            inverses.push(DomainCommandEnvelope {
                command_type: CONFIGURE_ENTITY.to_owned(),
                payload: serde_json::to_value(ConfigureEntity {
                    entity_id: previous.id,
                    enabled: previous.enabled,
                    target: previous.target,
                })
                .map_err(|error| payload("/inverse", error))?,
            });
        }
        inverses.reverse();
        Ok(DomainMutation {
            document: working,
            results,
            changes,
            inverse: DomainBatchCommand {
                schema_version: DOMAIN_BATCH_SCHEMA_VERSION,
                pack_id: batch.pack_id.clone(),
                scenario_schema_version: batch.scenario_schema_version,
                label: batch.label.clone(),
                commands: inverses,
            },
        })
    }

    fn compile(
        &self,
        document: &ScenarioDocument,
        context: &CompileContext,
    ) -> Result<PlanningProblem, DomainPackError> {
        if context.cancellation.is_cancelled() {
            return Err(DomainPackError::Cancelled);
        }
        require_pack(document)?;
        build_problem(document, context, &parse_entities(document)?)
    }

    fn project(
        &self,
        problem: &PlanningProblem,
        candidate: &CandidateValues,
        solution_id: SolutionId,
    ) -> Result<NormalizedSolution, DomainPackError> {
        if problem.metadata.pack_id.as_str() != OFFICIAL_TEST_PACK_ID {
            return Err(DomainPackError::Contract(
                "planning problem pack mismatch".to_owned(),
            ));
        }
        project_candidate(
            problem,
            candidate,
            solution_id,
            eutheto_planning_ir::PlanningIrLimitsV1::DEFAULT,
        )
        .map_err(contract)
    }

    fn verification_scope(
        &self,
        document: &ScenarioDocument,
        scenario_revision: u64,
    ) -> Result<VerificationScope, DomainPackError> {
        let required_rules = parse_entities(document)?
            .into_values()
            .map(|entity| {
                Ok(RequiredRuleBinding {
                    rule_id: RuleId::from_uuid(entity.id.as_uuid()),
                    semantic_hash: blake3_hex(
                        &serde_json::to_vec(&("official.test.required_target.v1", &entity))
                            .map_err(|error| payload("/domain/entities", error))?,
                    ),
                })
            })
            .collect::<Result<Vec<_>, DomainPackError>>()?;
        VerificationScope::new(document.scenario_id, scenario_revision, required_rules)
            .map_err(contract)
    }

    fn verify(
        &self,
        document: &ScenarioDocument,
        solution: &NormalizedSolution,
        context: &VerificationContextV1,
        authoritative_score: &ScoreVector,
    ) -> Result<VerificationReport, DomainPackError> {
        validate_verification_context(*self, document, solution, context)?;
        let evaluations = evaluate_required_rules(document, solution)?;
        VerificationReport::new(
            context,
            evaluations,
            authoritative_score.clone(),
            Vec::new(),
            BTreeMap::new(),
        )
        .map_err(contract)
    }

    fn score(
        &self,
        document: &ScenarioDocument,
        solution: &NormalizedSolution,
    ) -> Result<ScoreVector, DomainPackError> {
        authoritative_score(document, solution)
    }

    fn export_portable(
        &self,
        document: &ScenarioDocument,
    ) -> Result<PortableDomainDocument, DomainPackError> {
        require_pack(document)?;
        let payload_value = serde_json::to_value(PortablePayload {
            schema_version: 1,
            entities: parse_entities(document)?
                .into_iter()
                .map(|(id, entity)| {
                    (
                        id.to_string(),
                        PortableEntity {
                            enabled: entity.enabled,
                            target: entity.target,
                        },
                    )
                })
                .collect(),
            extensions: document
                .extensions
                .iter()
                .filter(|(key, _)| key.starts_with("nonsemantic."))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        })
        .map_err(|error| payload("/portable", error))?;
        validate_contract_value(
            &generated_catalog()?.portable_schema,
            &payload_value,
            ContractJsonLimits::DEFAULT,
        )?;
        Ok(PortableDomainDocument {
            pack_id: document.domain_pack.id.clone(),
            schema_version: 1,
            required_capabilities: [PORTABLE_CAPABILITY.to_owned()].into_iter().collect(),
            payload: payload_value,
        })
    }

    fn migrate_portable(
        &self,
        document: HistoricalPortableDomainDocument,
    ) -> Result<PortableDomainDocument, DomainPackError> {
        if document.pack_id.as_str() != OFFICIAL_TEST_PACK_ID {
            return Err(DomainPackError::PackUnavailable(
                document.pack_id.to_string(),
            ));
        }
        if document.schema_version != 0 {
            return Err(DomainPackError::UnsupportedVersion(document.schema_version));
        }
        if !document.required_capabilities.is_empty() {
            return Err(DomainPackError::Contract(
                "unknown historical semantic portable capability".to_owned(),
            ));
        }
        let historical: HistoricalPortablePayloadV0 = serde_json::from_value(document.payload)
            .map_err(|error| payload("/portable", error))?;
        if historical.schema_version != 0 {
            return Err(DomainPackError::UnsupportedVersion(
                historical.schema_version,
            ));
        }
        validate_portable_extensions(&historical.extensions)?;
        let payload_value = serde_json::to_value(PortablePayload {
            schema_version: 1,
            entities: historical.entities,
            extensions: historical.extensions,
        })
        .map_err(|error| payload("/portable", error))?;
        validate_contract_value(
            &generated_catalog()?.portable_schema,
            &payload_value,
            ContractJsonLimits::DEFAULT,
        )?;
        Ok(PortableDomainDocument {
            pack_id: document.pack_id,
            schema_version: 1,
            required_capabilities: [PORTABLE_CAPABILITY.to_owned()].into_iter().collect(),
            payload: payload_value,
        })
    }

    fn import_portable(
        &self,
        document: &PortableDomainDocument,
        context: &PortableImportContext,
    ) -> Result<ScenarioDocument, DomainPackError> {
        if document.pack_id.as_str() != OFFICIAL_TEST_PACK_ID {
            return Err(DomainPackError::PackUnavailable(
                document.pack_id.to_string(),
            ));
        }
        if document.schema_version != 1 {
            return Err(DomainPackError::UnsupportedVersion(document.schema_version));
        }
        let expected_capabilities: BTreeSet<_> =
            [PORTABLE_CAPABILITY.to_owned()].into_iter().collect();
        if document.required_capabilities != expected_capabilities {
            return Err(DomainPackError::Contract(
                "unknown or missing semantic portable capability".to_owned(),
            ));
        }
        validate_contract_value(
            &generated_catalog()?.portable_schema,
            &document.payload,
            ContractJsonLimits::DEFAULT,
        )?;
        let portable: PortablePayload = serde_json::from_value(document.payload.clone())
            .map_err(|error| payload("/portable", error))?;
        if portable.schema_version != 1 {
            return Err(DomainPackError::UnsupportedVersion(portable.schema_version));
        }
        validate_portable_extensions(&portable.extensions)?;
        let mut result = context.scenario_shell.clone();
        require_pack(&result)?;
        result.domain = ScenarioDomain::default();
        for (id, entity) in portable.entities {
            let id = PersonId::from_str(&id).map_err(contract)?;
            check_target(entity.target)?;
            result.domain.entities.insert(
                id,
                serde_json::to_value(TestEntity {
                    id,
                    enabled: entity.enabled,
                    target: entity.target,
                })
                .map_err(|error| payload("/domain/entities", error))?,
            );
        }
        result.domain_pack = DomainPackRef {
            id: document.pack_id.clone(),
            schema_version: 1,
        };
        result
            .extensions
            .retain(|key, _| !key.starts_with("nonsemantic."));
        result.extensions.extend(portable.extensions);
        Ok(result)
    }

    fn build_share_result(
        &self,
        document: &ScenarioDocument,
        accepted: &AcceptedResult,
        options: ShareResultOptions,
    ) -> Result<DomainShareResult, DomainPackError> {
        require_pack(document)?;
        accepted.validate().map_err(contract)?;
        if accepted.solution.pack_id.as_str() != OFFICIAL_TEST_PACK_ID
            || accepted.solution.scenario_id != document.scenario_id
        {
            return Err(DomainPackError::Contract(
                "accepted result scenario mismatch".to_owned(),
            ));
        }
        let _ = options;
        let assignments: Result<Vec<_>, DomainPackError> = accepted
            .solution
            .assignments
            .iter()
            .map(|assignment| {
                let value = match assignment.value {
                    AssignmentValue::Boolean(value) => Value::Bool(value),
                    AssignmentValue::Integer(value) => Value::from(value),
                    AssignmentValue::Absent => Value::Null,
                    AssignmentValue::Interval(_) => {
                        return Err(DomainPackError::Contract(
                            "test Share Result does not support interval assignments".to_owned(),
                        ));
                    }
                };
                Ok(json!({ "assignmentId": assignment.id.as_str(), "value": value }))
            })
            .collect();
        let payload_value = json!({ "schemaVersion": 1, "assignments": assignments? });
        validate_contract_value(
            &generated_catalog()?.share_result_schema,
            &payload_value,
            ContractJsonLimits::DEFAULT,
        )?;
        Ok(DomainShareResult {
            pack_id: document.domain_pack.id.clone(),
            schema_version: 1,
            payload: payload_value,
        })
    }

    fn build_view(
        &self,
        document: &ScenarioDocument,
        solution: Option<&NormalizedSolution>,
        view_id: &str,
    ) -> Result<eutheto_domain_api::DomainView, DomainPackError> {
        require_pack(document)?;
        if !generated_catalog()?
            .ui
            .result_views
            .iter()
            .any(|view| view.id == view_id)
        {
            return Err(DomainPackError::InvalidPayload {
                path: "/viewId".to_owned(),
                message: format!("unknown view {view_id}"),
            });
        }
        Ok(eutheto_domain_api::DomainView {
            view_id: view_id.to_owned(),
            data: json!({
                "entityCount": document.domain.entities.len(),
                "assignmentCount": solution.map_or(0, |value| value.assignments.len()),
            }),
        })
    }

    fn explain(
        &self,
        document: &ScenarioDocument,
        solution: Option<&NormalizedSolution>,
        request_id: &str,
    ) -> Result<DomainExplanation, DomainPackError> {
        require_pack(document)?;
        if request_id.is_empty() || request_id.len() > 160 {
            return Err(DomainPackError::InvalidPayload {
                path: "/requestId".to_owned(),
                message: "request ID must contain 1..=160 bytes".to_owned(),
            });
        }
        Ok(DomainExplanation {
            message_key: "official.test.explanation.summary".to_owned(),
            parameters: [
                ("requestId".to_owned(), Value::String(request_id.to_owned())),
                (
                    "assignmentCount".to_owned(),
                    Value::from(solution.map_or(0, |value| value.assignments.len())),
                ),
            ]
            .into_iter()
            .collect(),
        })
    }
}

fn generated_catalog() -> Result<DomainCatalog, DomainPackError> {
    let generated: GeneratedContract =
        serde_json::from_str(OFFICIAL_TEST_PACK_CONTRACT_JSON).map_err(contract)?;
    validate_generated_contract(&generated)?;
    let GeneratedContract {
        pack,
        commands,
        portable_schema,
        share_result_schema,
        ai_tools,
        ui_manifest,
        ..
    } = generated;
    let commands = command_descriptors(commands);
    let ai_tools = ai_tool_descriptors(ai_tools, &commands)?;
    Ok(DomainCatalog {
        pack_id: PackId::new(OFFICIAL_TEST_PACK_ID).map_err(contract)?,
        scenario_schema_version: pack.latest_schema_version,
        portable_schema,
        share_result_schema,
        commands,
        ai_tools,
        ui: domain_ui_manifest(ui_manifest),
    })
}

fn validate_generated_contract(generated: &GeneratedContract) -> Result<(), DomainPackError> {
    let generated_ids: Vec<_> = generated
        .commands
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    if generated.schema_version == 1
        && generated.pack.id == OFFICIAL_TEST_PACK_ID
        && generated.pack.pack_version == OFFICIAL_TEST_PACK_VERSION
        && generated.pack.latest_schema_version == 1
        && generated.pack.portable_schema_version == 1
        && generated.pack.share_result_schema_version == 1
        && generated_ids.as_slice() == OFFICIAL_TEST_COMMAND_IDS
    {
        Ok(())
    } else {
        Err(DomainPackError::CatalogMismatch(
            "generated official-test constants".to_owned(),
        ))
    }
}

fn command_descriptors(commands: Vec<GeneratedCommand>) -> Vec<CommandDescriptor> {
    commands
        .into_iter()
        .map(|command| CommandDescriptor {
            id: command.id,
            title: command.title,
            description: command.description,
            risk: command.risk,
            reversibility: command.reversibility,
            ai_grouping_allowed: command.ai_grouping_allowed,
            payload_schema: command.payload_schema,
            result_schema: command.result_schema,
            change_schema: command.change_schema,
            valid_examples: command.valid_examples,
            invalid_examples: command.invalid_examples,
        })
        .collect()
}

fn ai_tool_descriptors(
    tools: Vec<GeneratedAiTool>,
    commands: &[CommandDescriptor],
) -> Result<Vec<AiToolDescriptor>, DomainPackError> {
    tools
        .into_iter()
        .map(|tool| {
            let command = commands
                .iter()
                .find(|command| command.id == tool.command_id)
                .ok_or_else(|| DomainPackError::UnknownCommand(tool.command_id.clone()))?;
            Ok(AiToolDescriptor {
                command_id: tool.command_id,
                name: tool.name,
                description: tool.description,
                input_schema: command.payload_schema.clone(),
                valid_examples: command.valid_examples.clone(),
            })
        })
        .collect()
}

fn domain_ui_manifest(ui: GeneratedUiManifest) -> DomainUiManifest {
    DomainUiManifest {
        setup_steps: ui
            .setup_steps
            .into_iter()
            .map(|item| ui_kind(item, "Configure a synthetic entity."))
            .collect(),
        entity_kinds: ui
            .entity_kinds
            .into_iter()
            .map(|item| ui_kind(item, "Synthetic Boolean/integer entity."))
            .collect(),
        rule_kinds: ui
            .rule_kinds
            .into_iter()
            .map(|item| ui_kind(item, "Enabled iff target is positive."))
            .collect(),
        goal_kinds: vec![kind(
            "official.test.goal.target",
            "official.test.goal.target.title",
            "Minimize target",
            "Minimize the bounded target total.",
        )],
        score_kinds: vec![ScoreDescriptor {
            id: SCORE_CATEGORY.to_owned(),
            title: text("official.test.score.target_total.title", "Target total"),
            minimize: true,
        }],
        provenance_kinds: vec![kind(
            "official.test.provenance.required-target",
            "official.test.provenance.required_target.title",
            "Required target",
            "Traces the synthetic enabled/target relation.",
        )],
        result_views: ui
            .result_views
            .into_iter()
            .map(|item| ui_kind(item, "Synthetic result summary."))
            .collect(),
        importers: ui
            .importers
            .into_iter()
            .map(|item| transfer(item, "Import portable test data"))
            .collect(),
        exporters: ui
            .exporters
            .into_iter()
            .map(|item| transfer(item, "Export portable test data"))
            .collect(),
    }
}

fn ui_kind(item: GeneratedUiItem, description: &str) -> KindDescriptor {
    KindDescriptor {
        id: item.id,
        title: LocalizedText {
            key: item.title_key,
            default_text: "Synthetic test contract".to_owned(),
        },
        description: text("official.test.ui.description", description),
    }
}

fn transfer(item: GeneratedUiItem, title: &str) -> TransferDescriptor {
    TransferDescriptor {
        id: item.id,
        title: LocalizedText {
            key: item.title_key,
            default_text: title.to_owned(),
        },
        schema_version: 1,
    }
}

fn kind(id: &str, key: &str, title: &str, description: &str) -> KindDescriptor {
    KindDescriptor {
        id: id.to_owned(),
        title: text(key, title),
        description: text(&format!("{key}.description"), description),
    }
}

fn text(key: &str, default_text: &str) -> LocalizedText {
    LocalizedText {
        key: key.to_owned(),
        default_text: default_text.to_owned(),
    }
}

fn require_pack(document: &ScenarioDocument) -> Result<(), DomainPackError> {
    if document.domain_pack.id.as_str() != OFFICIAL_TEST_PACK_ID {
        return Err(DomainPackError::PackUnavailable(
            document.domain_pack.id.to_string(),
        ));
    }
    if document.domain_pack.schema_version != 1 {
        return Err(DomainPackError::UnsupportedVersion(
            document.domain_pack.schema_version,
        ));
    }
    Ok(())
}

fn validate_portable_extensions(
    extensions: &BTreeMap<String, Value>,
) -> Result<(), DomainPackError> {
    if let Some(extension) = extensions
        .keys()
        .find(|key| !key.starts_with("nonsemantic."))
    {
        return Err(DomainPackError::Contract(format!(
            "unknown semantic extension {extension}"
        )));
    }
    Ok(())
}

fn parse_entities(
    document: &ScenarioDocument,
) -> Result<BTreeMap<PersonId, TestEntity>, DomainPackError> {
    require_pack(document)?;
    let mut entities = BTreeMap::new();
    for (id, value) in &document.domain.entities {
        let entity: TestEntity = serde_json::from_value(value.clone())
            .map_err(|error| payload(&format!("/domain/entities/{id}"), error))?;
        if entity.id != *id {
            return Err(DomainPackError::Contract(format!(
                "entity {id} map key/id mismatch"
            )));
        }
        check_target(entity.target)?;
        entities.insert(*id, entity);
    }
    Ok(entities)
}

fn check_target(target: i64) -> Result<(), DomainPackError> {
    if (0..=10).contains(&target) {
        Ok(())
    } else {
        Err(DomainPackError::InvalidPayload {
            path: "/target".to_owned(),
            message: "target must be between 0 and 10".to_owned(),
        })
    }
}

fn validation_report(document: &ScenarioDocument) -> DomainValidationReport {
    let issues = match parse_entities(document) {
        Err(error) => vec![ValidationIssue {
            code: "official.test.invalid_document".to_owned(),
            severity: ValidationSeverity::Error,
            message: error.to_string(),
            field_path: Some("/domain/entities".to_owned()),
            resource: None,
        }],
        Ok(entities) => entities
            .into_iter()
            .filter(|(_, entity)| entity.enabled != (entity.target >= 1))
            .map(|(id, _)| ValidationIssue {
                code: "official.test.inconsistent_values".to_owned(),
                severity: ValidationSeverity::Error,
                message: "enabled must equal target >= 1".to_owned(),
                field_path: Some(format!("/domain/entities/{id}")),
                resource: None,
            })
            .collect(),
    };
    DomainValidationReport { issues }
}

fn validate_batch(
    document: &ScenarioDocument,
    batch: &DomainBatchCommand,
) -> Result<(), DomainPackError> {
    batch.validate_bounds()?;
    if batch.pack_id != document.domain_pack.id {
        return Err(DomainPackError::PackUnavailable(batch.pack_id.to_string()));
    }
    if batch.scenario_schema_version != document.domain_pack.schema_version {
        return Err(DomainPackError::UnsupportedVersion(
            batch.scenario_schema_version,
        ));
    }
    if let Some(command) = batch
        .commands
        .iter()
        .find(|command| command.command_type != CONFIGURE_ENTITY)
    {
        return Err(DomainPackError::UnknownCommand(
            command.command_type.clone(),
        ));
    }
    Ok(())
}

struct ProblemParts {
    variables: Vec<Variable>,
    constraints: Vec<ConstraintRecord>,
    objective_terms: Vec<ObjectiveTerm>,
    projections: Vec<SolutionProjection>,
    provenance: Vec<ProvenanceRecord>,
}

impl ProblemParts {
    fn with_capacity(entity_count: usize) -> Self {
        Self {
            variables: Vec::with_capacity(entity_count * 2),
            constraints: Vec::with_capacity(entity_count * 3),
            objective_terms: Vec::with_capacity(entity_count),
            projections: Vec::with_capacity(entity_count * 2),
            provenance: Vec::with_capacity(entity_count * 3 + 1),
        }
    }

    fn add_entity(
        &mut self,
        id: &PersonId,
        entity: &TestEntity,
        preference_provenance: &ProvenanceId,
    ) -> Result<(), DomainPackError> {
        let symbols = EntityProblemSymbols::new(id)?;
        self.variables.extend(entity_variables(&symbols)?);
        self.constraints
            .extend(entity_constraints(&symbols, entity)?);
        self.objective_terms
            .push(entity_objective(&symbols, preference_provenance)?);
        self.projections.extend(entity_projections(&symbols)?);
        self.provenance.extend(entity_provenance(symbols));
        Ok(())
    }
}

struct EntityProblemSymbols {
    suffix: String,
    bool_id: BoolVariableId,
    int_id: IntVariableId,
    fact: ProvenanceId,
    rule: ProvenanceId,
    projection: ProvenanceId,
    entity_ref: DomainEntityRef,
}

impl EntityProblemSymbols {
    fn new(id: &PersonId) -> Result<Self, DomainPackError> {
        let suffix = id.to_string();
        Ok(Self {
            bool_id: BoolVariableId::new(format!("official_test.enabled.{suffix}"))
                .map_err(contract)?,
            int_id: IntVariableId::new(format!("official_test.target.{suffix}"))
                .map_err(contract)?,
            fact: ProvenanceId::new(format!("official_test.fact.{suffix}")).map_err(contract)?,
            rule: ProvenanceId::new(format!("official_test.rule.{suffix}")).map_err(contract)?,
            projection: ProvenanceId::new(format!("official_test.projection.{suffix}"))
                .map_err(contract)?,
            entity_ref: DomainEntityRef {
                kind: DomainEntityKindId::new(ENTITY_KIND).map_err(contract)?,
                id: DomainEntityId::new(format!("official.test.entity.{suffix}"))
                    .map_err(contract)?,
            },
            suffix,
        })
    }
}

fn entity_variables(symbols: &EntityProblemSymbols) -> Result<[Variable; 2], DomainPackError> {
    let domain = IntDomain::new(vec![InclusiveRange { start: 0, end: 10 }]).map_err(contract)?;
    Ok([
        Variable::Boolean(BoolVariable {
            id: symbols.bool_id.clone(),
            provenance: symbols.fact.clone(),
        }),
        Variable::Integer(IntVariable {
            id: symbols.int_id.clone(),
            domain,
            provenance: symbols.fact.clone(),
        }),
    ])
}

fn entity_constraints(
    symbols: &EntityProblemSymbols,
    entity: &TestEntity,
) -> Result<[ConstraintRecord; 3], DomainPackError> {
    let required_tag = || ConstraintTag::new("official_test.required").map_err(contract);
    Ok([
        ConstraintRecord {
            id: PlanningConstraintId::new(format!(
                "official_test.fixed_enabled.{}",
                symbols.suffix
            ))
            .map_err(contract)?,
            body: Constraint::bool_and(vec![if entity.enabled {
                eutheto_planning_ir::Literal::positive(symbols.bool_id.clone())
            } else {
                eutheto_planning_ir::Literal::negative(symbols.bool_id.clone())
            }]),
            enforcement: Vec::new(),
            provenance: symbols.fact.clone(),
            tags: vec![required_tag()?],
        },
        ConstraintRecord {
            id: PlanningConstraintId::new(format!("official_test.fixed_target.{}", symbols.suffix))
                .map_err(contract)?,
            body: Constraint::LinearComparison(LinearComparison {
                expression: target_expression(&symbols.int_id)?,
                op: ComparisonOp::Equal,
                rhs: entity.target,
            }),
            enforcement: Vec::new(),
            provenance: symbols.fact.clone(),
            tags: vec![required_tag()?],
        },
        ConstraintRecord {
            id: PlanningConstraintId::new(format!(
                "official_test.required_target.{}",
                symbols.suffix
            ))
            .map_err(contract)?,
            body: Constraint::ReifiedLinearComparison {
                literal: eutheto_planning_ir::Literal::positive(symbols.bool_id.clone()),
                comparison: LinearComparison {
                    expression: target_expression(&symbols.int_id)?,
                    op: ComparisonOp::GreaterOrEqual,
                    rhs: 1,
                },
            },
            enforcement: Vec::new(),
            provenance: symbols.rule.clone(),
            tags: vec![required_tag()?],
        },
    ])
}

fn target_expression(int_id: &IntVariableId) -> Result<LinearExpression, DomainPackError> {
    LinearExpression::new(
        vec![LinearTerm {
            variable: int_id.clone(),
            coefficient: 1,
        }],
        0,
    )
    .map_err(contract)
}

fn entity_objective(
    symbols: &EntityProblemSymbols,
    preference_provenance: &ProvenanceId,
) -> Result<ObjectiveTerm, DomainPackError> {
    Ok(ObjectiveTerm {
        id: ObjectiveTermId::new(format!("official_test.target_penalty.{}", symbols.suffix))
            .map_err(contract)?,
        expression: target_expression(&symbols.int_id)?,
        kind: ObjectiveTermKind::Penalty,
        category: ScoreCategoryId::new(SCORE_CATEGORY).map_err(contract)?,
        provenance: preference_provenance.clone(),
    })
}

fn entity_projections(
    symbols: &EntityProblemSymbols,
) -> Result<[SolutionProjection; 2], DomainPackError> {
    Ok([
        SolutionProjection {
            id: ProjectionId::new(format!("official_test.project_enabled.{}", symbols.suffix))
                .map_err(contract)?,
            assignment_id: DomainAssignmentId::new(format!(
                "official.test.assignment.enabled.{}",
                symbols.suffix
            ))
            .map_err(contract)?,
            entity: symbols.entity_ref.clone(),
            required: true,
            expression: ProjectionExpression::Boolean(symbols.bool_id.clone()),
            provenance: symbols.projection.clone(),
        },
        SolutionProjection {
            id: ProjectionId::new(format!("official_test.project_target.{}", symbols.suffix))
                .map_err(contract)?,
            assignment_id: DomainAssignmentId::new(format!(
                "official.test.assignment.target.{}",
                symbols.suffix
            ))
            .map_err(contract)?,
            entity: symbols.entity_ref.clone(),
            required: true,
            expression: ProjectionExpression::Integer(symbols.int_id.clone()),
            provenance: symbols.projection.clone(),
        },
    ])
}

fn entity_provenance(symbols: EntityProblemSymbols) -> [ProvenanceRecord; 3] {
    [
        provenance_record(
            symbols.fact,
            ProvenanceSourceKind::Fact,
            format!("official.test.entity.{}", symbols.suffix),
            symbols.entity_ref.clone(),
        ),
        provenance_record(
            symbols.rule,
            ProvenanceSourceKind::RequiredRule,
            "official.test.required-target".to_owned(),
            symbols.entity_ref.clone(),
        ),
        provenance_record(
            symbols.projection,
            ProvenanceSourceKind::Projection,
            format!("official.test.projection.{}", symbols.suffix),
            symbols.entity_ref,
        ),
    ]
}

fn compile_metadata(
    context: &CompileContext,
) -> Result<BTreeMap<MetadataKey, ProvenanceParameter>, DomainPackError> {
    context
        .semantic_metadata
        .iter()
        .map(|(key, value)| {
            Ok((
                MetadataKey::new(key.clone()).map_err(contract)?,
                ProvenanceParameter::Text(value.clone()),
            ))
        })
        .collect()
}

fn build_problem(
    document: &ScenarioDocument,
    context: &CompileContext,
    entities: &BTreeMap<PersonId, TestEntity>,
) -> Result<PlanningProblem, DomainPackError> {
    let preference_provenance =
        ProvenanceId::new("official_test.preference.target").map_err(contract)?;
    let mut parts = ProblemParts::with_capacity(entities.len());
    for (id, entity) in entities {
        parts.add_entity(id, entity, &preference_provenance)?;
    }
    parts.provenance.push(ProvenanceRecord {
        id: preference_provenance.clone(),
        source_kind: ProvenanceSourceKind::Preference,
        source_id: "official.test.preference.target".to_owned(),
        entity_refs: Vec::new(),
        message_key: "official.test.provenance.target_preference".to_owned(),
        parameters: BTreeMap::new(),
        parent: None,
    });
    let upper_bound = i64::try_from(entities.len())
        .ok()
        .and_then(|count| count.checked_mul(10))
        .ok_or_else(|| DomainPackError::Contract("objective bound overflow".to_owned()))?;
    let mut problem = PlanningProblem {
        schema_version: PLANNING_IR_SCHEMA_VERSION,
        variables: parts.variables,
        constraints: parts.constraints,
        objectives: ObjectivePlan {
            levels: vec![ObjectiveLevel {
                id: ObjectiveLevelId::new(SCORE_LEVEL).map_err(contract)?,
                direction: OptimizationDirection::Minimize,
                lower_bound: 0,
                upper_bound,
                terms: parts.objective_terms,
                provenance: preference_provenance,
            }],
        },
        assumptions: Vec::new(),
        projections: parts.projections,
        provenance: parts.provenance,
        metadata: PlanningMetadata {
            pack_id: document.domain_pack.id.clone(),
            scenario_id: document.scenario_id,
            scenario_revision: context.scenario_revision,
            projection_version: PROJECTION_SCHEMA_VERSION,
            compiler_id: CompilerId::new("official_test.compiler").map_err(contract)?,
            compiler_version: OFFICIAL_TEST_PACK_VERSION.to_owned(),
            compile_metadata: compile_metadata(context)?,
            display_text: BTreeMap::new(),
        },
        declared_capabilities: [
            Capability::ReifiedLinearComparison,
            Capability::BoolAnd,
            Capability::LinearComparison,
            Capability::ObjectivePenalty,
            Capability::BooleanProjection,
            Capability::IntegerProjection,
        ]
        .into_iter()
        .collect(),
        split_authorization: None,
    };
    problem.canonicalize().map_err(contract)?;
    validate(&problem, context.planning_limits).map_err(contract)?;
    Ok(problem)
}

fn provenance_record(
    id: ProvenanceId,
    source_kind: ProvenanceSourceKind,
    source_id: String,
    entity: DomainEntityRef,
) -> ProvenanceRecord {
    ProvenanceRecord {
        id,
        source_kind,
        source_id,
        entity_refs: vec![entity],
        message_key: "official.test.provenance.record".to_owned(),
        parameters: BTreeMap::new(),
        parent: None,
    }
}

fn validate_verification_context(
    pack: crate::OfficialTestPack,
    document: &ScenarioDocument,
    solution: &NormalizedSolution,
    context: &VerificationContextV1,
) -> Result<(), DomainPackError> {
    context.validate().map_err(contract)?;
    let document_hash =
        blake3_hex(&serde_json::to_vec(document).map_err(|error| payload("/document", error))?);
    let normalized_solution_hash = solution.canonical_hash().map_err(contract)?;
    let scope = pack.verification_scope(document, solution.scenario_revision)?;
    if context.scenario_id != document.scenario_id
        || context.evaluated_revision != solution.scenario_revision
        || context.document_hash != document_hash
        || context.normalized_solution_hash != normalized_solution_hash
        || context.verification_scope_checksum != scope.checksum
    {
        return Err(contract("verification context binding mismatch"));
    }
    Ok(())
}

fn evaluate_required_rules(
    document: &ScenarioDocument,
    solution: &NormalizedSolution,
) -> Result<Vec<RuleEvaluation>, DomainPackError> {
    require_pack(document)?;
    solution.validate().map_err(contract)?;
    if solution.pack_id != document.domain_pack.id || solution.scenario_id != document.scenario_id {
        return Err(DomainPackError::Contract(
            "solution scenario mismatch".to_owned(),
        ));
    }
    let entities = parse_entities(document)?;
    let assignments: BTreeMap<_, _> = solution
        .assignments
        .iter()
        .map(|assignment| (assignment.id.as_str().to_owned(), assignment))
        .collect();
    let expected_assignment_ids: BTreeSet<_> = entities
        .keys()
        .flat_map(|id| {
            let suffix = id.to_string();
            [
                format!("official.test.assignment.enabled.{suffix}"),
                format!("official.test.assignment.target.{suffix}"),
            ]
        })
        .collect();
    if assignments
        .keys()
        .any(|id| !expected_assignment_ids.contains(id))
    {
        return Err(DomainPackError::Contract(
            "solution contains an unknown domain assignment".to_owned(),
        ));
    }

    let enabled_fact = VerificationFactId::new("official.test.fact.enabled").map_err(contract)?;
    let target_fact = VerificationFactId::new("official.test.fact.target").map_err(contract)?;
    let mut evaluations = Vec::with_capacity(entities.len());
    for (id, entity) in &entities {
        let suffix = id.to_string();
        let entity_ref = DomainEntityRef {
            kind: DomainEntityKindId::new(ENTITY_KIND).map_err(contract)?,
            id: DomainEntityId::new(format!("official.test.entity.{suffix}")).map_err(contract)?,
        };
        let enabled_assignment =
            assignments.get(&format!("official.test.assignment.enabled.{suffix}"));
        let target_assignment =
            assignments.get(&format!("official.test.assignment.target.{suffix}"));
        if enabled_assignment
            .into_iter()
            .chain(target_assignment)
            .any(|assignment| assignment.entity != entity_ref)
        {
            return Err(contract("solution assignment entity mismatch"));
        }
        let enabled = enabled_assignment.map(|assignment| &assignment.value);
        let target = target_assignment.map(|assignment| &assignment.value);
        let satisfied = matches!(
            (enabled, target),
            (
                Some(AssignmentValue::Boolean(enabled)),
                Some(AssignmentValue::Integer(target))
            ) if *enabled == entity.enabled
                && *target == entity.target
                && *enabled == (*target >= 1)
        );
        evaluations.push(RuleEvaluation {
            rule_id: RuleId::from_uuid(id.as_uuid()),
            satisfied,
            affected_entities: vec![entity_ref],
            message_key: "official.test.verify.required_target".to_owned(),
            expected: [
                (
                    enabled_fact.clone(),
                    VerificationValue::Boolean(entity.enabled),
                ),
                (
                    target_fact.clone(),
                    VerificationValue::Integer(entity.target),
                ),
            ]
            .into_iter()
            .collect(),
            observed: [
                (enabled_fact.clone(), assignment_verification_value(enabled)),
                (target_fact.clone(), assignment_verification_value(target)),
            ]
            .into_iter()
            .collect(),
            evidence: Vec::new(),
        });
    }
    Ok(evaluations)
}

fn authoritative_score(
    document: &ScenarioDocument,
    solution: &NormalizedSolution,
) -> Result<ScoreVector, DomainPackError> {
    require_pack(document)?;
    solution.validate().map_err(contract)?;
    if solution.pack_id != document.domain_pack.id || solution.scenario_id != document.scenario_id {
        return Err(DomainPackError::Contract(
            "solution scenario mismatch".to_owned(),
        ));
    }
    let entities = parse_entities(document)?;
    let assignments: BTreeMap<_, _> = solution
        .assignments
        .iter()
        .map(|assignment| (assignment.id.as_str().to_owned(), &assignment.value))
        .collect();
    let expected_assignment_ids: BTreeSet<_> = entities
        .keys()
        .flat_map(|id| {
            let suffix = id.to_string();
            [
                format!("official.test.assignment.enabled.{suffix}"),
                format!("official.test.assignment.target.{suffix}"),
            ]
        })
        .collect();
    if assignments
        .keys()
        .any(|id| !expected_assignment_ids.contains(id))
    {
        return Err(contract("solution contains an unknown domain assignment"));
    }

    let mut feasibility = 0_i64;
    let mut total = 0_i64;
    for (id, entity) in &entities {
        let suffix = id.to_string();
        let enabled = assignments.get(&format!("official.test.assignment.enabled.{suffix}"));
        let target = assignments.get(&format!("official.test.assignment.target.{suffix}"));
        let satisfied = matches!(
            (enabled, target),
            (
                Some(AssignmentValue::Boolean(enabled)),
                Some(AssignmentValue::Integer(target))
            ) if *enabled == entity.enabled
                && *target == entity.target
                && *enabled == (*target >= 1)
        );
        if !satisfied {
            feasibility = feasibility
                .checked_add(1)
                .ok_or_else(|| contract("score feasibility overflow"))?;
        }
        if let Some(AssignmentValue::Integer(value)) = target {
            total = total
                .checked_add(*value)
                .ok_or_else(|| contract("score overflow"))?;
        }
    }
    Ok(ScoreVector {
        feasibility,
        levels: vec![ScoreLevelValue {
            level_id: ScoreLevelId::new(SCORE_LEVEL).map_err(contract)?,
            value: total,
            direction: OptimizationDirection::Minimize,
            category_breakdown: [(
                ScoreCategoryId::new(SCORE_CATEGORY).map_err(contract)?,
                total,
            )]
            .into_iter()
            .collect(),
        }],
    })
}

fn assignment_verification_value(value: Option<&AssignmentValue>) -> VerificationValue {
    match value {
        Some(AssignmentValue::Boolean(value)) => VerificationValue::Boolean(*value),
        Some(AssignmentValue::Integer(value)) => VerificationValue::Integer(*value),
        Some(AssignmentValue::Interval(value)) => VerificationValue::Text(format!(
            "interval:{}:{}:{}",
            value.start, value.duration, value.end
        )),
        Some(AssignmentValue::Absent) => VerificationValue::Text("absent".to_owned()),
        None => VerificationValue::Text("missing".to_owned()),
    }
}

fn payload(path: &str, error: impl std::fmt::Display) -> DomainPackError {
    DomainPackError::InvalidPayload {
        path: path.to_owned(),
        message: error.to_string(),
    }
}

fn contract(error: impl std::fmt::Display) -> DomainPackError {
    DomainPackError::Contract(error.to_string())
}
