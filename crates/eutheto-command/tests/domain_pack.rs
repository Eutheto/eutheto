use eutheto_command::{OFFICIAL_TEST_PACK_ID, OfficialTestPack, official_registry};
use eutheto_domain_api::{
    CompileContext, ContractJsonLimits, DOMAIN_BATCH_SCHEMA_VERSION, DomainBatchCommand,
    DomainPack, DomainPackError, DomainPackRegistry, DomainShareResult,
    HistoricalPortableDomainDocument, PortableImportContext, ShareResultOptions,
    validate_contract_value,
};
use eutheto_domain_ir::{
    AcceptedResult, AssignmentValue, NormalizedSolution, VerificationContextV1, blake3_hex,
};
use eutheto_planning_ir::{
    CandidateValues, PlanningIrLimitsV1, PlanningProblem, ProvenanceSourceKind, Variable,
    canonical_ir_hash, summarize, validate,
};
use eutheto_types::{
    CancellationToken, DomainCommandEnvelope, PackId, PersonId, RuleId, ScenarioDocument,
    SolutionId,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::str::FromStr;

const SCENARIO_ID: &str = "0195a5e4-7c00-7000-8000-000000000001";
const ENTITY_ID: &str = "018f25a7-8b3c-7d11-8000-000000000001";
const SOLUTION_ID: &str = "0195a5e4-7c00-7000-8000-000000000002";
const COMMAND_ID: &str = "official.test.configure_entity";

fn document() -> Result<ScenarioDocument, serde_json::Error> {
    serde_json::from_value(json!({
        "format": "eutheto/scenario",
        "formatVersion": 1,
        "scenarioId": SCENARIO_ID,
        "domainPack": { "id": OFFICIAL_TEST_PACK_ID, "schemaVersion": 1 },
        "metadata": {
            "title": "Pack conformance",
            "description": "",
            "createdAt": "2026-08-29T12:00:00Z",
            "updatedAt": "2026-08-29T12:00:00Z"
        },
        "settings": {
            "timeZone": "UTC",
            "locale": "en-US",
            "units": "metric",
            "horizon": {
                "start": "2026-08-29T12:00:00Z",
                "end": "2026-09-05T12:00:00Z"
            },
            "gapPolicy": "reject",
            "overlapPolicy": "earlier"
        },
        "domain": {
            "entities": {
                (ENTITY_ID): { "id": ENTITY_ID, "enabled": true, "target": 3 }
            },
            "rules": {},
            "preferences": {},
            "lockedAssignments": {}
        },
        "extensions": {}
    }))
}

fn command(enabled: bool, target: i64) -> DomainCommandEnvelope {
    DomainCommandEnvelope {
        command_type: COMMAND_ID.to_owned(),
        payload: json!({ "entityId": ENTITY_ID, "enabled": enabled, "target": target }),
    }
}

fn batch(commands: Vec<DomainCommandEnvelope>) -> Result<DomainBatchCommand, Box<dyn Error>> {
    Ok(DomainBatchCommand {
        schema_version: DOMAIN_BATCH_SCHEMA_VERSION,
        pack_id: PackId::new(OFFICIAL_TEST_PACK_ID)?,
        scenario_schema_version: 1,
        label: Some("Conformance batch".to_owned()),
        commands,
    })
}

fn context() -> CompileContext {
    CompileContext {
        scenario_revision: 7,
        semantic_metadata: BTreeMap::new(),
        cancellation: CancellationToken::new(),
        planning_limits: PlanningIrLimitsV1::DEFAULT,
    }
}
fn verification_context(
    pack: &dyn DomainPack,
    document: &ScenarioDocument,
    problem: &PlanningProblem,
    solution: &NormalizedSolution,
) -> Result<VerificationContextV1, Box<dyn Error>> {
    let scope = pack.verification_scope(document, solution.scenario_revision)?;
    Ok(VerificationContextV1::new(
        document.scenario_id,
        solution.scenario_revision,
        blake3_hex(&serde_json::to_vec(document)?),
        canonical_ir_hash(problem, PlanningIrLimitsV1::DEFAULT)?,
        solution.canonical_hash()?,
        scope.checksum,
    )?)
}

#[test]
fn registry_rejects_duplicates_and_missing_metadata() -> Result<(), Box<dyn Error>> {
    let duplicate = DomainPackRegistry::builder()
        .register(OfficialTestPack)
        .register(OfficialTestPack)
        .build();
    assert!(matches!(duplicate, Err(DomainPackError::DuplicatePack(_))));

    let mut descriptor = DomainPack::descriptor(&OfficialTestPack)?;
    descriptor.display_name.default_text.clear();
    assert!(matches!(
        descriptor.validate(),
        Err(DomainPackError::MissingMetadata(_))
    ));
    Ok(())
}

#[test]
fn catalog_consumes_generated_examples_and_is_complete() -> Result<(), Box<dyn Error>> {
    let registry = official_registry()?;
    let descriptors: Vec<_> = registry.descriptors().collect();
    assert_eq!(descriptors.len(), 1);
    assert!(descriptors[0].synthetic_test_only);
    assert!(descriptors[0].scenario_versions.supports(1));
    assert!(!descriptors[0].scenario_versions.supports(2));
    let catalog = registry
        .catalog(&PackId::new(OFFICIAL_TEST_PACK_ID)?)
        .ok_or("registered catalog is required")?;
    catalog.validate()?;
    assert_eq!(catalog.commands.len(), 1);
    assert_eq!(catalog.commands[0].id, COMMAND_ID);
    assert_eq!(
        catalog.ai_tools[0].input_schema,
        catalog.commands[0].payload_schema
    );
    assert!(!catalog.ui.setup_steps.is_empty());
    assert!(!catalog.ui.entity_kinds.is_empty());
    assert!(!catalog.ui.rule_kinds.is_empty());
    assert!(!catalog.ui.goal_kinds.is_empty());
    assert!(!catalog.ui.score_kinds.is_empty());
    assert!(!catalog.ui.provenance_kinds.is_empty());
    assert!(!catalog.ui.result_views.is_empty());
    assert!(!catalog.ui.importers.is_empty());
    assert!(!catalog.ui.exporters.is_empty());
    for example in &catalog.commands[0].valid_examples {
        validate_contract_value(
            &catalog.commands[0].payload_schema,
            example,
            ContractJsonLimits::DEFAULT,
        )?;
    }
    for example in &catalog.commands[0].invalid_examples {
        assert!(
            validate_contract_value(
                &catalog.commands[0].payload_schema,
                example,
                ContractJsonLimits::DEFAULT,
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn command_validation_rejects_unknown_version_command_and_field_atomically()
-> Result<(), Box<dyn Error>> {
    let pack = OfficialTestPack;
    let source = document()?;

    let mut unknown_version = batch(vec![command(true, 3)])?;
    unknown_version.scenario_schema_version = 2;
    assert!(matches!(
        pack.apply_batch(&source, &unknown_version),
        Err(DomainPackError::UnsupportedVersion(2))
    ));

    let unknown_command = DomainCommandEnvelope {
        command_type: "official.test.future".to_owned(),
        payload: json!({}),
    };
    assert!(matches!(
        pack.apply_batch(&source, &batch(vec![unknown_command])?),
        Err(DomainPackError::UnknownCommand(_))
    ));

    let invalid_field = DomainCommandEnvelope {
        command_type: COMMAND_ID.to_owned(),
        payload: json!({
            "entityId": ENTITY_ID,
            "enabled": true,
            "target": 3,
            "future": true
        }),
    };
    let two = batch(vec![command(true, 3), invalid_field])?;
    assert!(matches!(
        pack.apply_batch(&source, &two),
        Err(DomainPackError::InvalidPayload { .. })
    ));
    assert_eq!(source, document()?);
    Ok(())
}

#[test]
fn batch_is_atomic_and_inverse_restores_exact_document() -> Result<(), Box<dyn Error>> {
    let pack = OfficialTestPack;
    let source = document()?;
    let applied = pack.apply_batch(&source, &batch(vec![command(true, 3), command(true, 4)])?)?;
    assert_eq!(applied.results.len(), 2);
    assert_eq!(applied.changes.len(), 2);
    assert_eq!(applied.inverse.commands.len(), 2);
    let restored = pack.apply_batch(&applied.document, &applied.inverse)?;
    assert_eq!(restored.document, source);
    Ok(())
}

#[test]
fn portable_and_share_contracts_round_trip() -> Result<(), Box<dyn Error>> {
    let pack = OfficialTestPack;
    let mut source = document()?;
    let portable = pack.export_portable(&source)?;
    let serialized = serde_json::to_vec(&portable)?;
    let decoded = serde_json::from_slice(&serialized)?;
    let imported = pack.import_portable(
        &decoded,
        &PortableImportContext {
            scenario_shell: source.clone(),
        },
    )?;
    assert_eq!(imported.domain, source.domain);
    let migrated = pack.migrate_portable(HistoricalPortableDomainDocument {
        pack_id: PackId::new(OFFICIAL_TEST_PACK_ID)?,
        schema_version: 0,
        required_capabilities: BTreeSet::default(),
        payload: json!({
            "schemaVersion": 0,
            "entities": {
                (ENTITY_ID): { "enabled": true, "target": 3 }
            }
        }),
    })?;
    assert_eq!(migrated.payload, portable.payload);

    let problem = pack.compile(&source, &context())?;
    let mut candidate = CandidateValues::default();
    for variable in &problem.variables {
        match variable {
            Variable::Boolean(value) => {
                candidate.booleans.insert(value.id.clone(), true);
            }
            Variable::Integer(value) => {
                candidate.integers.insert(value.id.clone(), 3);
            }
            Variable::Interval(_) => {}
        }
    }
    let solution = pack.project(&problem, &candidate, SolutionId::from_str(SOLUTION_ID)?)?;
    let authoritative_score = pack.score(&source, &solution)?;
    let verification_context = verification_context(&pack, &source, &problem, &solution)?;
    let verification = pack.verify(
        &source,
        &solution,
        &verification_context,
        &authoritative_score,
    )?;
    let accepted = AcceptedResult::new(solution, verification)?;
    source.extensions.insert(
        "nonsemantic.source-only".to_owned(),
        json!({"private": "must-not-be-shared"}),
    );
    let shared = pack.build_share_result(&source, &accepted, ShareResultOptions::default())?;
    let share_bytes = serde_json::to_vec(&shared)?;
    let share_round_trip: DomainShareResult = serde_json::from_slice(&share_bytes)?;
    assert_eq!(share_round_trip, shared);
    validate_contract_value(
        &pack.catalog()?.share_result_schema,
        &shared.payload,
        ContractJsonLimits::DEFAULT,
    )?;
    assert!(shared.payload.get("source").is_none());
    assert!(shared.payload.get("extensions").is_none());
    assert!(!serde_json::to_string(&shared.payload)?.contains("must-not-be-shared"));
    let mut unaccepted = accepted.clone();
    unaccepted.verification.accepted = false;
    assert!(
        pack.build_share_result(&source, &unaccepted, ShareResultOptions::default())
            .is_err()
    );
    Ok(())
}

#[test]
fn portable_nonsemantic_extensions_survive_current_and_historical_round_trips()
-> Result<(), Box<dyn Error>> {
    let pack = OfficialTestPack;
    let extensions = BTreeMap::from([
        (
            "nonsemantic.example.alpha".to_owned(),
            json!({
                "nested": [1, "two", true, null],
                "object": {"z": 3, "a": "exact"}
            }),
        ),
        (
            "nonsemantic.example.empty".to_owned(),
            json!({"array": [], "object": {}}),
        ),
    ]);
    let mut source = document()?;
    source.extensions = extensions.clone();

    let current = pack.export_portable(&source)?;
    assert_eq!(
        current.payload["extensions"],
        serde_json::to_value(&extensions)?
    );
    let imported = pack.import_portable(
        &current,
        &PortableImportContext {
            scenario_shell: document()?,
        },
    )?;
    assert_eq!(imported.domain, source.domain);
    assert_eq!(imported.extensions, extensions);
    assert_eq!(pack.export_portable(&imported)?.payload, current.payload);

    let migrated = pack.migrate_portable(HistoricalPortableDomainDocument {
        pack_id: PackId::new(OFFICIAL_TEST_PACK_ID)?,
        schema_version: 0,
        required_capabilities: BTreeSet::default(),
        payload: json!({
            "schemaVersion": 0,
            "entities": {
                (ENTITY_ID): { "enabled": true, "target": 3 }
            },
            "extensions": extensions
        }),
    })?;
    assert_eq!(
        migrated.payload["extensions"],
        current.payload["extensions"]
    );
    let migrated_import = pack.import_portable(
        &migrated,
        &PortableImportContext {
            scenario_shell: document()?,
        },
    )?;
    assert_eq!(migrated_import.domain, source.domain);
    assert_eq!(migrated_import.extensions, source.extensions);
    assert_eq!(
        pack.export_portable(&migrated_import)?.payload,
        migrated.payload
    );
    Ok(())
}

#[test]
fn portable_semantic_extensions_are_rejected_in_current_and_historical_data()
-> Result<(), Box<dyn Error>> {
    let pack = OfficialTestPack;
    let clean = document()?;
    let mut current = pack.export_portable(&clean)?;
    current.payload["extensions"] = json!({
        "semantic.required-rule": {"meaning": "must not be ignored"}
    });
    assert!(matches!(
        pack.import_portable(
            &current,
            &PortableImportContext {
                scenario_shell: clean
            }
        ),
        Err(DomainPackError::Contract(message))
            if message.contains("semantic.required-rule")
    ));

    assert!(matches!(
        pack.migrate_portable(HistoricalPortableDomainDocument {
            pack_id: PackId::new(OFFICIAL_TEST_PACK_ID)?,
            schema_version: 0,
            required_capabilities: BTreeSet::default(),
            payload: json!({
                "schemaVersion": 0,
                "entities": {
                    (ENTITY_ID): { "enabled": true, "target": 3 }
                },
                "extensions": {
                    "semantic.required-rule": {"meaning": "must not be ignored"}
                }
            }),
        }),
        Err(DomainPackError::Contract(message))
            if message.contains("semantic.required-rule")
    ));
    Ok(())
}

fn satisfying_candidate(problem: &PlanningProblem) -> CandidateValues {
    let mut candidate = CandidateValues::default();
    for variable in &problem.variables {
        match variable {
            Variable::Boolean(value) => {
                candidate.booleans.insert(value.id.clone(), true);
            }
            Variable::Integer(value) => {
                candidate.integers.insert(value.id.clone(), 3);
            }
            Variable::Interval(_) => {}
        }
    }
    candidate
}

#[test]
// This contract test keeps compilation, projection, verification, and score determinism together.
#[allow(clippy::too_many_lines)]
fn compile_project_verify_and_score_are_deterministic() -> Result<(), Box<dyn Error>> {
    let pack = OfficialTestPack;
    let source = document()?;
    let limits = PlanningIrLimitsV1::DEFAULT;
    let verification_scope = pack.verification_scope(&source, context().scenario_revision)?;
    assert_eq!(
        verification_scope,
        pack.verification_scope(&source, context().scenario_revision)?
    );
    assert_eq!(verification_scope.required_rules.len(), 1);
    assert_eq!(
        verification_scope.required_rules[0].rule_id,
        RuleId::from_uuid(PersonId::from_str(ENTITY_ID)?.as_uuid())
    );
    let first = pack.compile(&source, &context())?;
    let second = pack.compile(&source, &context())?;
    validate(&first, limits)?;
    validate(&second, limits)?;
    let first_summary = summarize(&first, limits)?;
    let second_summary = summarize(&second, limits)?;
    assert_eq!(first, second);
    assert_eq!(first_summary, second_summary);
    assert_eq!(first.variables.len(), 2);
    assert_eq!(first.constraints.len(), 3);
    assert_eq!(first.projections.len(), 2);
    assert_eq!(first.provenance.len(), 4);
    assert_eq!(first_summary.bool_variable_count, 1);
    assert_eq!(first_summary.int_variable_count, 1);
    assert_eq!(first_summary.constraint_count, 3);
    assert_eq!(first_summary.objective_term_count, 1);
    assert_eq!(first_summary.projection_count, 2);
    assert_eq!(first_summary.provenance_count, 4);
    let required_provenance = first
        .provenance
        .iter()
        .find(|record| record.source_kind == ProvenanceSourceKind::RequiredRule)
        .ok_or("required-rule provenance missing")?;
    assert_eq!(
        required_provenance.source_id,
        verification_scope.required_rules[0].rule_id.to_string()
    );
    assert_eq!(
        first.declared_capabilities,
        first_summary.manifest.required_capabilities()
    );
    assert_eq!(first.objectives.levels[0].lower_bound, 0);
    assert_eq!(first.objectives.levels[0].upper_bound, 10);
    let mut renamed_source = source.clone();
    renamed_source.metadata.title = "Renamed display-only scenario".to_owned();
    let renamed = pack.compile(&renamed_source, &context())?;
    assert_eq!(
        summarize(&renamed, limits)?.canonical_ir_hash,
        first_summary.canonical_ir_hash
    );

    let mut display_variant = first.clone();
    display_variant.metadata.display_text.insert(
        "official.test.display.label".to_owned(),
        "A different localized display label".to_owned(),
    );
    validate(&display_variant, limits)?;
    assert_eq!(
        summarize(&display_variant, limits)?.canonical_ir_hash,
        first_summary.canonical_ir_hash
    );

    let candidate = satisfying_candidate(&first);
    let solution = pack.project(&first, &candidate, SolutionId::from_str(SOLUTION_ID)?)?;
    let projection_evidence = format!("official_test.projection.{ENTITY_ID}");
    assert!(solution.assignments.iter().all(|assignment| {
        assignment.evidence.len() == 1 && assignment.evidence[0].as_str() == projection_evidence
    }));
    let authoritative_score = pack.score(&source, &solution)?;
    let context_for_solution = verification_context(&pack, &source, &first, &solution)?;
    let mut unsatisfied = solution.clone();
    let target = unsatisfied
        .assignments
        .iter_mut()
        .find(|assignment| assignment.id.as_str().contains(".target."))
        .ok_or("target assignment missing")?;
    target.value = AssignmentValue::Integer(4);
    let unsatisfied_score = pack.score(&source, &unsatisfied)?;
    let unsatisfied_context = verification_context(&pack, &source, &first, &unsatisfied)?;
    let report = pack.verify(
        &source,
        &solution,
        &context_for_solution,
        &authoritative_score,
    )?;
    assert!(report.accepted);
    assert_eq!(report.required_rule_results.len(), 1);
    assert!(
        report
            .required_rule_results
            .iter()
            .all(|evaluation| evaluation.satisfied)
    );
    assert_eq!(authoritative_score.feasibility, 0);
    assert_eq!(authoritative_score.levels[0].value, 3);
    assert_eq!(report.score, authoritative_score);
    assert_eq!(
        report.verification_scope_checksum,
        verification_scope.checksum
    );
    assert_eq!(
        report.required_rule_results[0].rule_id,
        verification_scope.required_rules[0].rule_id
    );
    assert_eq!(report.required_rule_results[0].evidence.len(), 1);
    assert_eq!(
        report.required_rule_results[0].evidence[0].as_str(),
        format!("official_test.rule.{ENTITY_ID}")
    );
    assert_eq!(
        report.required_rule_results[0].evidence[0].as_str(),
        required_provenance.id.as_str()
    );
    assert_eq!(
        report,
        pack.verify(&source, &solution, &context_for_solution, &report.score)?
    );

    let unsatisfied_report = pack.verify(
        &source,
        &unsatisfied,
        &unsatisfied_context,
        &unsatisfied_score,
    )?;
    assert!(!unsatisfied_report.accepted);
    assert_eq!(unsatisfied_report.required_rule_results.len(), 1);
    assert!(!unsatisfied_report.required_rule_results[0].satisfied);
    assert!(unsatisfied_report.warnings.is_empty());
    Ok(())
}

#[test]
fn unknown_internal_and_portable_fields_are_rejected() -> Result<(), Box<dyn Error>> {
    let pack = OfficialTestPack;
    let mut source = document()?;
    source.domain.entities.insert(
        PersonId::from_str(ENTITY_ID)?,
        json!({ "id": ENTITY_ID, "enabled": true, "target": 3, "future": true }),
    );
    assert!(!pack.validate_fast(&source).issues.is_empty());
    assert!(pack.compile(&source, &context()).is_err());

    let clean = document()?;
    let mut portable = pack.export_portable(&clean)?;
    portable.payload["future"] = Value::Bool(true);
    assert!(
        pack.import_portable(
            &portable,
            &PortableImportContext {
                scenario_shell: clean
            }
        )
        .is_err()
    );
    Ok(())
}
