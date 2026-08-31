use eutheto_command::{
    AppliedCommand, CODE_INVALID_RECORD_SHAPE, CODE_RECORD_ID_MISMATCH, CommandError,
    OFFICIAL_TEST_PACK_ID, apply_command,
};
use eutheto_types::{
    ActorRef, AddEntity, CommandBatch, CommandEnvelope, CommandId, CommandSource, PersonId,
    Revision, ScenarioCommand, ScenarioDocument, UpdateEntity,
};
use proptest::char::range;
use proptest::collection::{btree_map, vec};
use proptest::prelude::*;
use proptest::test_runner::{RngAlgorithm, RngSeed, TestCaseError};
use serde_json::{Value, json};
use std::fmt::Display;
use std::str::FromStr;

const SCENARIO_ID: &str = "0195a5e4-7c00-7000-8000-000000000001";
const COMMAND_ID: &str = "0195a5e4-7c00-7000-8000-000000000002";

fn deterministic_config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: None,
        cases: 128,
        max_shrink_iters: 4_096,
        rng_algorithm: RngAlgorithm::ChaCha,
        rng_seed: RngSeed::Fixed(0x4555_5448_4554_4f01),
        ..ProptestConfig::default()
    }
}

fn case_error(error: impl Display) -> TestCaseError {
    TestCaseError::fail(error.to_string())
}

fn document() -> Result<ScenarioDocument, TestCaseError> {
    serde_json::from_value(json!({
        "format": "eutheto/scenario",
        "formatVersion": 1,
        "scenarioId": SCENARIO_ID,
        "domainPack": { "id": OFFICIAL_TEST_PACK_ID, "schemaVersion": 1 },
        "metadata": {
            "title": "Command properties",
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
            "entities": {},
            "rules": {},
            "preferences": {},
            "lockedAssignments": {}
        },
        "extensions": { "vendor.example": { "preserved": true } }
    }))
    .map_err(case_error)
}

fn person_id(index: u8) -> Result<PersonId, TestCaseError> {
    let suffix = u64::from(index).saturating_add(0x100);
    PersonId::from_str(&format!("0195a5e4-7c00-7000-8000-{suffix:012x}")).map_err(case_error)
}

fn envelope(
    document: &ScenarioDocument,
    revision: Revision,
    command: ScenarioCommand,
) -> Result<CommandEnvelope, TestCaseError> {
    Ok(CommandEnvelope {
        command_id: CommandId::from_str(COMMAND_ID).map_err(case_error)?,
        scenario_id: document.scenario_id,
        expected_revision: revision,
        actor: ActorRef {
            actor_id: Some("property-test".to_owned()),
            display_name: "Property Test".to_owned(),
        },
        source: CommandSource::System,
        command,
    })
}

fn apply_case(
    document: &ScenarioDocument,
    revision: Revision,
    command: ScenarioCommand,
) -> Result<AppliedCommand, TestCaseError> {
    let command_envelope = envelope(document, revision, command)?;
    apply_command(document, revision, &command_envelope).map_err(case_error)
}

fn canonical_document(document: &ScenarioDocument) -> Result<Vec<u8>, TestCaseError> {
    serde_json::to_vec(document).map_err(case_error)
}

fn record_strategy() -> impl Strategy<Value = Value> {
    (vec(range('a', 'z'), 0..24), any::<i16>(), any::<bool>()).prop_map(|(name, rank, active)| {
        json!({
            "active": active,
            "name": String::from_iter(name),
            "rank": rank
        })
    })
}

#[derive(Clone, Debug)]
enum EntityOperation {
    Add(Value),
    Update { before: Value, after: Value },
    Remove(Value),
}

fn entity_operation_strategy() -> BoxedStrategy<EntityOperation> {
    prop_oneof![
        record_strategy().prop_map(EntityOperation::Add),
        (record_strategy(), record_strategy())
            .prop_map(|(before, after)| EntityOperation::Update { before, after }),
        record_strategy().prop_map(EntityOperation::Remove),
    ]
    .boxed()
}

#[derive(Clone, Debug)]
enum InvalidRecord {
    NonObject(Value),
    MismatchedId(Value),
}

fn invalid_record_strategy() -> BoxedStrategy<InvalidRecord> {
    let non_object = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i32>().prop_map(|number| json!(number)),
        vec(any::<u8>(), 0..8).prop_map(|items| json!(items)),
        vec(range('a', 'z'), 0..16)
            .prop_map(|characters| Value::String(String::from_iter(characters))),
    ];
    let mismatched_id = prop_oneof![
        Just(json!({ "id": "0195a5e4-7c00-7000-8000-00000000ffff" })),
        any::<i32>().prop_map(|id| json!({ "id": id })),
    ];

    prop_oneof![
        non_object.prop_map(InvalidRecord::NonObject),
        mismatched_id.prop_map(InvalidRecord::MismatchedId),
    ]
    .boxed()
}

proptest! {
    #![proptest_config(deterministic_config())]

    #[test]
    fn command_and_inverse_restore_canonical_document(
        operation in entity_operation_strategy(),
        revision_value in 0_u64..10_000,
    ) {
        let mut scenario = document()?;
        let entity_id = person_id(1)?;
        let command = match operation {
            EntityOperation::Add(value) => ScenarioCommand::AddEntity(AddEntity {
                entity_id,
                value,
            }),
            EntityOperation::Update { before, after } => {
                scenario.domain.entities.insert(entity_id, before);
                ScenarioCommand::UpdateEntity(UpdateEntity {
                    entity_id,
                    value: after,
                })
            }
            EntityOperation::Remove(value) => {
                scenario.domain.entities.insert(entity_id, value);
                ScenarioCommand::RemoveEntity(eutheto_types::RemoveEntity { entity_id })
            }
        };
        let original = canonical_document(&scenario)?;
        let applied = apply_case(&scenario, Revision::new(revision_value), command)?;
        let inverse = applied.result.inverse.clone().ok_or_else(|| {
            TestCaseError::fail("successful command did not return an inverse")
        })?;
        let restored = apply_case(
            &applied.document,
            applied.result.new_revision,
            inverse,
        )?;

        prop_assert_eq!(canonical_document(&restored.document)?, original);
    }

    #[test]
    fn batch_inverse_reverses_leaf_order_and_restores_document(
        records in btree_map(0_u8..32, record_strategy(), 1..12),
        reverse in any::<bool>(),
        rotation in 0_usize..32,
        revision_value in 0_u64..10_000,
    ) {
        let scenario = document()?;
        let original = canonical_document(&scenario)?;
        let mut ordered_records: Vec<_> = records.into_iter().collect();
        if reverse {
            ordered_records.reverse();
        }
        let record_count = ordered_records.len();
        ordered_records.rotate_left(rotation % record_count);

        let mut command_ids = Vec::with_capacity(record_count);
        let mut commands = Vec::with_capacity(record_count);
        for (index, value) in ordered_records {
            let entity_id = person_id(index)?;
            command_ids.push(entity_id);
            commands.push(ScenarioCommand::AddEntity(AddEntity { entity_id, value }));
        }
        let applied = apply_case(
            &scenario,
            Revision::new(revision_value),
            ScenarioCommand::ApplyBatch(CommandBatch {
                label: Some("property batch".to_owned()),
                commands,
            }),
        )?;
        let inverse = applied.result.inverse.clone().ok_or_else(|| {
            TestCaseError::fail("successful batch did not return an inverse")
        })?;
        let ScenarioCommand::ApplyBatch(inverse_batch) = inverse else {
            return Err(TestCaseError::fail("batch inverse was not a batch"));
        };
        let mut inverse_ids = Vec::with_capacity(record_count);
        for command in &inverse_batch.commands {
            let ScenarioCommand::RemoveEntity(remove) = command else {
                return Err(TestCaseError::fail("add-only batch inverse was not remove-only"));
            };
            inverse_ids.push(remove.entity_id);
        }
        command_ids.reverse();
        prop_assert_eq!(inverse_ids, command_ids);

        let restored = apply_case(
            &applied.document,
            applied.result.new_revision,
            ScenarioCommand::ApplyBatch(inverse_batch),
        )?;
        prop_assert_eq!(canonical_document(&restored.document)?, original);
    }

    #[test]
    fn application_result_is_stable_under_map_insertion_order(
        records in btree_map(0_u8..32, record_strategy(), 1..12),
        revision_value in 0_u64..10_000,
    ) {
        let mut forward = document()?;
        let mut reverse = document()?;
        let ordered_records: Vec<_> = records.into_iter().collect();
        for (index, value) in &ordered_records {
            forward.domain.entities.insert(person_id(*index)?, value.clone());
        }
        for (index, value) in ordered_records.iter().rev() {
            reverse.domain.entities.insert(person_id(*index)?, value.clone());
        }

        let mut updates = Vec::with_capacity(ordered_records.len());
        for (index, _) in &ordered_records {
            let entity_id = person_id(*index)?;
            updates.push(ScenarioCommand::UpdateEntity(UpdateEntity {
                entity_id,
                value: json!({ "index": index, "updated": true }),
            }));
        }
        let command = ScenarioCommand::ApplyBatch(CommandBatch {
            label: None,
            commands: updates,
        });
        let revision = Revision::new(revision_value);
        let forward_result = apply_case(&forward, revision, command.clone())?;
        let reverse_result = apply_case(&reverse, revision, command)?;

        prop_assert_eq!(forward_result, reverse_result);
    }

    #[test]
    fn invalid_record_shapes_return_checked_stable_errors(
        invalid in invalid_record_strategy(),
        revision_value in 0_u64..10_000,
    ) {
        let scenario = document()?;
        let before = canonical_document(&scenario)?;
        let entity_id = person_id(1)?;
        let (value, expected_code, expected_suffix) = match invalid {
            InvalidRecord::NonObject(value) => (value, CODE_INVALID_RECORD_SHAPE, ""),
            InvalidRecord::MismatchedId(value) => (value, CODE_RECORD_ID_MISMATCH, "/id"),
        };
        let command_envelope = envelope(
            &scenario,
            Revision::new(revision_value),
            ScenarioCommand::AddEntity(AddEntity { entity_id, value }),
        )?;
        let result = apply_command(&scenario, Revision::new(revision_value), &command_envelope);

        match result {
            Err(CommandError::Validation { code, path, .. }) => {
                prop_assert_eq!(code, expected_code);
                prop_assert_eq!(path, format!("/command/value{expected_suffix}"));
            }
            Err(error) => {
                return Err(TestCaseError::fail(format!(
                    "invalid record returned non-validation error: {error}"
                )));
            }
            Ok(_) => {
                return Err(TestCaseError::fail("invalid record shape was accepted"));
            }
        }
        prop_assert_eq!(canonical_document(&scenario)?, before);
    }
}
