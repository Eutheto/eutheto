//! Pure scenario command application.
//!
//! This crate deliberately owns no persistence. Callers pass an immutable
//! document and revision and receive a replacement document plus journal-ready
//! metadata. The input is never modified, including on batch failure.

use eutheto_types::{
    AddEntity, AddRule, AssignmentId, Change, ChangeKind, ChangeSet, CommandBatch, CommandEnvelope,
    CommandResult, DomainCommandEnvelope, LockAssignment, PersonId, PortableJsonLimits, Revision,
    RuleId, ScenarioCommand, ScenarioDocument, SetPreference, UnlockAssignment, UpdateEntity,
    UpdateRule, ValidationDelta, ValidationIssue, validate_nonsecret_portable_json,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// The only concrete domain pack available in Phase 01.
pub const OFFICIAL_TEST_PACK_ID: &str = "official.test";
/// Protects pure application from adversarially deep nested batches.
pub const MAX_BATCH_DEPTH: usize = 8;
/// Bounds total leaf commands in one atomic batch.
pub const MAX_BATCH_COMMANDS: usize = 1_000;

pub const CODE_BATCH_DEPTH_EXCEEDED: &str = "command.batch_depth_exceeded";
pub const CODE_BATCH_TOO_LARGE: &str = "command.batch_too_large";
pub const CODE_DUPLICATE_ENTITY: &str = "command.duplicate_entity";
pub const CODE_DUPLICATE_LOCK: &str = "command.duplicate_assignment_lock";
pub const CODE_DUPLICATE_RULE: &str = "command.duplicate_rule";
pub const CODE_EMPTY_BATCH: &str = "command.empty_batch";
pub const CODE_INVALID_RECORD_SHAPE: &str = "command.invalid_record_shape";
pub const CODE_MISSING_ENTITY: &str = "command.missing_entity";
pub const CODE_MISSING_LOCK: &str = "command.missing_assignment_lock";
pub const CODE_MISSING_PREFERENCE: &str = "command.missing_preference";
pub const CODE_MISSING_RULE: &str = "command.missing_rule";
pub const CODE_RECORD_ID_MISMATCH: &str = "command.record_id_mismatch";
pub const CODE_PROHIBITED_DATA: &str = "command.prohibited_data";

/// Successful, side-effect-free application of one envelope.
#[derive(Clone, Debug, PartialEq)]
pub struct AppliedCommand {
    /// Complete replacement document.
    pub document: ScenarioDocument,
    /// Revision, changes, validation delta, and generated inverse.
    pub result: CommandResult,
    /// Deterministic journal/UI summary.
    pub summary: String,
    /// Stable command type metadata.
    pub command_type: String,
}

/// Stable failure returned by pure application.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CommandError {
    /// The command targets a different scenario.
    #[error(
        "command scenario {command_scenario_id} does not match document scenario {document_scenario_id}"
    )]
    ScenarioMismatch {
        command_scenario_id: String,
        document_scenario_id: String,
    },
    /// Optimistic revision check failed.
    #[error("revision conflict: expected {expected}, actual {actual}")]
    Conflict { expected: u64, actual: u64 },
    /// A stable structural or command validation rule failed.
    #[error("validation {code} at {path}: {message}")]
    Validation {
        code: &'static str,
        path: String,
        message: String,
    },
    /// No registered pack can apply the command.
    #[error("unsupported command {command_type} for domain pack {pack_id}")]
    Unsupported {
        pack_id: String,
        command_type: String,
    },
    /// A domain command payload did not match its declared command type.
    #[error("invalid payload for domain command {command_type}: {message}")]
    InvalidDomainPayload {
        command_type: String,
        message: String,
    },
    /// The revision cannot be incremented.
    #[error("revision overflow at {revision}")]
    RevisionOverflow { revision: u64 },
}

impl CommandError {
    /// Machine-stable validation/dispatch code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ScenarioMismatch { .. } => "command.scenario_mismatch",
            Self::Conflict { .. } => "command.revision_conflict",
            Self::Validation { code, .. } => code,
            Self::Unsupported { .. } => "command.unsupported",
            Self::InvalidDomainPayload { .. } => "command.invalid_domain_payload",
            Self::RevisionOverflow { .. } => "command.revision_overflow",
        }
    }
}

/// Effect returned by a domain-pack implementation for one non-batch command.
#[derive(Clone, Debug, PartialEq)]
pub struct PackCommandEffect {
    pub changes: Vec<Change>,
    pub inverse: ScenarioCommand,
    pub summary: String,
    pub command_type: String,
}

/// Pure seam implemented by future domain packs.
pub trait DomainPackApplication {
    /// Stable pack identifier handled by this implementation.
    fn pack_id(&self) -> &str;

    /// Apply one non-batch command to a private working copy.
    ///
    /// The engine discards the copy if this method returns an error.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError::Validation`] when the command references a
    /// missing or duplicate entity, rule, or lock, or violates record-shape
    /// invariants.  Returns [`CommandError::Unsupported`] when the pack
    /// cannot handle the supplied command variant.
    fn apply(
        &self,
        document: &mut ScenarioDocument,
        command: &ScenarioCommand,
    ) -> Result<PackCommandEffect, CommandError>;
}

/// Fast, deterministic validation seam implemented by future packs.
pub trait FastValidator {
    /// Report current issues in deterministic order.
    fn validate(&self, document: &ScenarioDocument) -> Vec<ValidationIssue>;
}

/// Phase-01 generic pack handler.
#[derive(Clone, Copy, Debug, Default)]
pub struct OfficialTestPack;

impl FastValidator for OfficialTestPack {
    fn validate(&self, _document: &ScenarioDocument) -> Vec<ValidationIssue> {
        Vec::new()
    }
}

impl DomainPackApplication for OfficialTestPack {
    fn pack_id(&self) -> &str {
        OFFICIAL_TEST_PACK_ID
    }

    fn apply(
        &self,
        document: &mut ScenarioDocument,
        command: &ScenarioCommand,
    ) -> Result<PackCommandEffect, CommandError> {
        apply_official_test_leaf(document, command)
    }
}

/// Apply using the sole concrete Phase-01 pack implementation.
///
/// The function performs no I/O, does not mutate `document`, increments the
/// revision exactly once (including for a batch), and returns no document on
/// failure.
///
/// # Errors
///
/// Propagates [`CommandError::ScenarioMismatch`],
/// [`CommandError::Conflict`], [`CommandError::Unsupported`], and
/// [`CommandError::RevisionOverflow`] for envelope/precondition
/// failures.  Returns [`CommandError::Validation`] when the batch
/// exceeds depth or size limits, or when any leaf command fails.
pub fn apply_command(
    document: &ScenarioDocument,
    current_revision: Revision,
    envelope: &CommandEnvelope,
) -> Result<AppliedCommand, CommandError> {
    let pack = OfficialTestPack;
    apply_command_with(document, current_revision, envelope, &pack, &pack)
}

/// Apply with explicitly supplied future pack and fast-validation seams.
///
/// # Errors
///
/// Returns [`CommandError::ScenarioMismatch`] when the envelope targets
/// a different scenario, [`CommandError::Conflict`] on revision
/// mismatch, [`CommandError::Unsupported`] when the pack id does not
/// match the document, and [`CommandError::Validation`] for structural
/// or batch-limit violations.  [`CommandError::RevisionOverflow`] is
/// returned when the revision counter cannot advance.
pub fn apply_command_with<P, V>(
    document: &ScenarioDocument,
    current_revision: Revision,
    envelope: &CommandEnvelope,
    pack: &P,
    validator: &V,
) -> Result<AppliedCommand, CommandError>
where
    P: DomainPackApplication + ?Sized,
    V: FastValidator + ?Sized,
{
    validate_safe_serialized(&envelope.command, "/command")?;
    if envelope.scenario_id != document.scenario_id {
        return Err(CommandError::ScenarioMismatch {
            command_scenario_id: envelope.scenario_id.to_string(),
            document_scenario_id: document.scenario_id.to_string(),
        });
    }
    if envelope.expected_revision != current_revision {
        return Err(CommandError::Conflict {
            expected: envelope.expected_revision.value(),
            actual: current_revision.value(),
        });
    }

    if document.domain_pack.id.as_str() != pack.pack_id() {
        return Err(CommandError::Unsupported {
            pack_id: document.domain_pack.id.to_string(),
            command_type: command_type(&envelope.command),
        });
    }

    validate_document_shape(document)?;
    let before_issues = validator.validate(document);
    let mut working = document.clone();
    let mut leaf_count = 0_usize;
    let effect = apply_nested(&mut working, &envelope.command, pack, 0, &mut leaf_count)?;
    validate_document_shape(&working)?;
    let after_issues = validator.validate(&working);
    let validation_delta = validation_delta(&before_issues, &after_issues);
    let new_revision =
        current_revision
            .checked_next()
            .map_err(|_| CommandError::RevisionOverflow {
                revision: current_revision.value(),
            })?;

    Ok(AppliedCommand {
        document: working,
        result: CommandResult {
            new_revision,
            change_set: ChangeSet {
                changes: effect.changes,
            },
            validation_delta,
            inverse: Some(effect.inverse),
        },
        summary: effect.summary,
        command_type: effect.command_type,
    })
}

fn apply_nested<P: DomainPackApplication + ?Sized>(
    document: &mut ScenarioDocument,
    command: &ScenarioCommand,
    pack: &P,
    depth: usize,
    leaf_count: &mut usize,
) -> Result<PackCommandEffect, CommandError> {
    if let ScenarioCommand::ApplyBatch(batch) = command {
        return apply_batch(document, batch, pack, depth, leaf_count);
    }
    *leaf_count = leaf_count.saturating_add(1);
    if *leaf_count > MAX_BATCH_COMMANDS {
        return Err(validation_error(
            CODE_BATCH_TOO_LARGE,
            "/command/commands",
            format!("batch may contain at most {MAX_BATCH_COMMANDS} commands"),
        ));
    }
    pack.apply(document, command)
}

fn apply_batch<P: DomainPackApplication + ?Sized>(
    document: &mut ScenarioDocument,
    batch: &CommandBatch,
    pack: &P,
    depth: usize,
    leaf_count: &mut usize,
) -> Result<PackCommandEffect, CommandError> {
    if batch.commands.is_empty() {
        return Err(validation_error(
            CODE_EMPTY_BATCH,
            "/command/commands",
            "a batch must contain at least one command",
        ));
    }
    if depth >= MAX_BATCH_DEPTH {
        return Err(validation_error(
            CODE_BATCH_DEPTH_EXCEEDED,
            "/command/commands",
            format!("batch nesting may not exceed {MAX_BATCH_DEPTH}"),
        ));
    }

    let mut changes = Vec::new();
    let mut inverses = Vec::with_capacity(batch.commands.len());
    for child in &batch.commands {
        let effect = apply_nested(document, child, pack, depth + 1, leaf_count)?;
        changes.extend(effect.changes);
        inverses.push(effect.inverse);
    }
    inverses.reverse();
    let count = batch.commands.len();
    let summary = match &batch.label {
        Some(label) => format!("{label} ({count} commands)"),
        None => format!("Apply batch ({count} commands)"),
    };
    Ok(PackCommandEffect {
        changes,
        inverse: ScenarioCommand::ApplyBatch(CommandBatch {
            label: batch.label.clone(),
            commands: inverses,
        }),
        summary,
        command_type: "apply_batch".to_owned(),
    })
}

fn apply_official_test_leaf(
    document: &mut ScenarioDocument,
    command: &ScenarioCommand,
) -> Result<PackCommandEffect, CommandError> {
    match command {
        ScenarioCommand::AddEntity(value) => add_entity(document, value),
        ScenarioCommand::UpdateEntity(value) => update_entity(document, value),
        ScenarioCommand::RemoveEntity(value) => remove_entity(document, value),
        ScenarioCommand::AddRule(value) => add_rule(document, value),
        ScenarioCommand::UpdateRule(value) => update_rule(document, value),
        ScenarioCommand::RemoveRule(value) => remove_rule(document, value),
        ScenarioCommand::SetPreference(value) => set_preference(document, value),
        ScenarioCommand::LockAssignment(value) => lock_assignment(document, value),
        ScenarioCommand::UnlockAssignment(value) => unlock_assignment(document, value),
        ScenarioCommand::ApplyDomainCommand(value) => apply_domain_command(document, value),
        ScenarioCommand::ApplyBatch(_) => Err(validation_error(
            CODE_INVALID_RECORD_SHAPE,
            "/command",
            "the command engine, not a pack, must apply batches",
        )),
    }
}

fn add_entity(
    document: &mut ScenarioDocument,
    command: &AddEntity,
) -> Result<PackCommandEffect, CommandError> {
    validate_record(&command.value, &command.entity_id, "/command/value")?;
    if document.domain.entities.contains_key(&command.entity_id) {
        return Err(validation_error(
            CODE_DUPLICATE_ENTITY,
            entity_path(&command.entity_id),
            "entity already exists",
        ));
    }
    document
        .domain
        .entities
        .insert(command.entity_id, command.value.clone());
    Ok(effect(
        ChangeKind::Added,
        entity_path(&command.entity_id),
        None,
        Some(command.value.clone()),
        ScenarioCommand::RemoveEntity(eutheto_types::RemoveEntity {
            entity_id: command.entity_id,
        }),
        format!("Add entity {}", command.entity_id),
        "add_entity",
    ))
}

fn update_entity(
    document: &mut ScenarioDocument,
    command: &UpdateEntity,
) -> Result<PackCommandEffect, CommandError> {
    validate_record(&command.value, &command.entity_id, "/command/value")?;
    let Some(previous) = document
        .domain
        .entities
        .insert(command.entity_id, command.value.clone())
    else {
        return Err(validation_error(
            CODE_MISSING_ENTITY,
            entity_path(&command.entity_id),
            "entity does not exist",
        ));
    };
    Ok(effect(
        ChangeKind::Updated,
        entity_path(&command.entity_id),
        Some(previous.clone()),
        Some(command.value.clone()),
        ScenarioCommand::UpdateEntity(UpdateEntity {
            entity_id: command.entity_id,
            value: previous,
        }),
        format!("Update entity {}", command.entity_id),
        "update_entity",
    ))
}

fn remove_entity(
    document: &mut ScenarioDocument,
    command: &eutheto_types::RemoveEntity,
) -> Result<PackCommandEffect, CommandError> {
    let Some(previous) = document.domain.entities.remove(&command.entity_id) else {
        return Err(validation_error(
            CODE_MISSING_ENTITY,
            entity_path(&command.entity_id),
            "entity does not exist",
        ));
    };
    Ok(effect(
        ChangeKind::Removed,
        entity_path(&command.entity_id),
        Some(previous.clone()),
        None,
        ScenarioCommand::AddEntity(AddEntity {
            entity_id: command.entity_id,
            value: previous,
        }),
        format!("Remove entity {}", command.entity_id),
        "remove_entity",
    ))
}

fn add_rule(
    document: &mut ScenarioDocument,
    command: &AddRule,
) -> Result<PackCommandEffect, CommandError> {
    validate_record(&command.value, &command.rule_id, "/command/value")?;
    if document.domain.rules.contains_key(&command.rule_id) {
        return Err(validation_error(
            CODE_DUPLICATE_RULE,
            rule_path(&command.rule_id),
            "rule already exists",
        ));
    }
    document
        .domain
        .rules
        .insert(command.rule_id, command.value.clone());
    Ok(effect(
        ChangeKind::Added,
        rule_path(&command.rule_id),
        None,
        Some(command.value.clone()),
        ScenarioCommand::RemoveRule(eutheto_types::RemoveRule {
            rule_id: command.rule_id,
        }),
        format!("Add rule {}", command.rule_id),
        "add_rule",
    ))
}

fn update_rule(
    document: &mut ScenarioDocument,
    command: &UpdateRule,
) -> Result<PackCommandEffect, CommandError> {
    validate_record(&command.value, &command.rule_id, "/command/value")?;
    let Some(previous) = document
        .domain
        .rules
        .insert(command.rule_id, command.value.clone())
    else {
        return Err(validation_error(
            CODE_MISSING_RULE,
            rule_path(&command.rule_id),
            "rule does not exist",
        ));
    };
    Ok(effect(
        ChangeKind::Updated,
        rule_path(&command.rule_id),
        Some(previous.clone()),
        Some(command.value.clone()),
        ScenarioCommand::UpdateRule(UpdateRule {
            rule_id: command.rule_id,
            value: previous,
        }),
        format!("Update rule {}", command.rule_id),
        "update_rule",
    ))
}

fn remove_rule(
    document: &mut ScenarioDocument,
    command: &eutheto_types::RemoveRule,
) -> Result<PackCommandEffect, CommandError> {
    let Some(previous) = document.domain.rules.remove(&command.rule_id) else {
        return Err(validation_error(
            CODE_MISSING_RULE,
            rule_path(&command.rule_id),
            "rule does not exist",
        ));
    };
    Ok(effect(
        ChangeKind::Removed,
        rule_path(&command.rule_id),
        Some(previous.clone()),
        None,
        ScenarioCommand::AddRule(AddRule {
            rule_id: command.rule_id,
            value: previous,
        }),
        format!("Remove rule {}", command.rule_id),
        "remove_rule",
    ))
}

fn set_preference(
    document: &mut ScenarioDocument,
    command: &SetPreference,
) -> Result<PackCommandEffect, CommandError> {
    let path = preference_path(&command.preference_id);
    let previous = match &command.value {
        Some(value) => {
            validate_record(value, &command.preference_id, "/command/value")?;
            document
                .domain
                .preferences
                .insert(command.preference_id, value.clone())
        }
        None => document.domain.preferences.remove(&command.preference_id),
    };
    if command.value.is_none() && previous.is_none() {
        return Err(validation_error(
            CODE_MISSING_PREFERENCE,
            path,
            "preference does not exist",
        ));
    }
    let kind = match (&previous, &command.value) {
        (None, Some(_)) => ChangeKind::Added,
        (Some(_), Some(_)) => ChangeKind::Updated,
        _ => ChangeKind::Removed,
    };
    let action = if command.value.is_some() {
        "Set"
    } else {
        "Clear"
    };
    Ok(effect(
        kind,
        preference_path(&command.preference_id),
        previous.clone(),
        command.value.clone(),
        ScenarioCommand::SetPreference(SetPreference {
            preference_id: command.preference_id,
            value: previous,
        }),
        format!("{action} preference {}", command.preference_id),
        "set_preference",
    ))
}

fn lock_assignment(
    document: &mut ScenarioDocument,
    command: &LockAssignment,
) -> Result<PackCommandEffect, CommandError> {
    validate_record(&command.value, &command.assignment_id, "/command/value")?;
    if document
        .domain
        .locked_assignments
        .contains_key(&command.assignment_id)
    {
        return Err(validation_error(
            CODE_DUPLICATE_LOCK,
            lock_path(&command.assignment_id),
            "assignment is already locked",
        ));
    }
    document
        .domain
        .locked_assignments
        .insert(command.assignment_id, command.value.clone());
    Ok(effect(
        ChangeKind::Locked,
        lock_path(&command.assignment_id),
        None,
        Some(command.value.clone()),
        ScenarioCommand::UnlockAssignment(UnlockAssignment {
            assignment_id: command.assignment_id,
        }),
        format!("Lock assignment {}", command.assignment_id),
        "lock_assignment",
    ))
}

fn unlock_assignment(
    document: &mut ScenarioDocument,
    command: &UnlockAssignment,
) -> Result<PackCommandEffect, CommandError> {
    let Some(previous) = document
        .domain
        .locked_assignments
        .remove(&command.assignment_id)
    else {
        return Err(validation_error(
            CODE_MISSING_LOCK,
            lock_path(&command.assignment_id),
            "assignment is not locked",
        ));
    };
    Ok(effect(
        ChangeKind::Unlocked,
        lock_path(&command.assignment_id),
        Some(previous.clone()),
        None,
        ScenarioCommand::LockAssignment(LockAssignment {
            assignment_id: command.assignment_id,
            value: previous,
        }),
        format!("Unlock assignment {}", command.assignment_id),
        "unlock_assignment",
    ))
}

fn apply_domain_command(
    document: &mut ScenarioDocument,
    envelope: &DomainCommandEnvelope,
) -> Result<PackCommandEffect, CommandError> {
    let command = match envelope.command_type.as_str() {
        "add_entity" => ScenarioCommand::AddEntity(decode_domain(envelope)?),
        "update_entity" => ScenarioCommand::UpdateEntity(decode_domain(envelope)?),
        "remove_entity" => ScenarioCommand::RemoveEntity(decode_domain(envelope)?),
        "add_rule" => ScenarioCommand::AddRule(decode_domain(envelope)?),
        "update_rule" => ScenarioCommand::UpdateRule(decode_domain(envelope)?),
        "remove_rule" => ScenarioCommand::RemoveRule(decode_domain(envelope)?),
        "set_preference" => ScenarioCommand::SetPreference(decode_domain(envelope)?),
        "lock_assignment" => ScenarioCommand::LockAssignment(decode_domain(envelope)?),
        "unlock_assignment" => ScenarioCommand::UnlockAssignment(decode_domain(envelope)?),
        _ => {
            return Err(CommandError::Unsupported {
                pack_id: OFFICIAL_TEST_PACK_ID.to_owned(),
                command_type: envelope.command_type.clone(),
            });
        }
    };
    let underlying = apply_official_test_leaf(document, &command)?;
    let inverse = domain_inverse(underlying.inverse)?;
    Ok(PackCommandEffect {
        changes: underlying.changes,
        inverse: ScenarioCommand::ApplyDomainCommand(inverse),
        summary: format!("Apply {} domain command", envelope.command_type),
        command_type: format!("domain.{}", envelope.command_type),
    })
}

fn decode_domain<T: DeserializeOwned>(envelope: &DomainCommandEnvelope) -> Result<T, CommandError> {
    serde_json::from_value(envelope.payload.clone()).map_err(|error| {
        CommandError::InvalidDomainPayload {
            command_type: envelope.command_type.clone(),
            message: error.to_string(),
        }
    })
}

fn domain_inverse(command: ScenarioCommand) -> Result<DomainCommandEnvelope, CommandError> {
    let command_type = command_type(&command);
    let payload = match command {
        ScenarioCommand::AddEntity(value) => encode_domain(value, &command_type)?,
        ScenarioCommand::UpdateEntity(value) => encode_domain(value, &command_type)?,
        ScenarioCommand::RemoveEntity(value) => encode_domain(value, &command_type)?,
        ScenarioCommand::AddRule(value) => encode_domain(value, &command_type)?,
        ScenarioCommand::UpdateRule(value) => encode_domain(value, &command_type)?,
        ScenarioCommand::RemoveRule(value) => encode_domain(value, &command_type)?,
        ScenarioCommand::SetPreference(value) => encode_domain(value, &command_type)?,
        ScenarioCommand::LockAssignment(value) => encode_domain(value, &command_type)?,
        ScenarioCommand::UnlockAssignment(value) => encode_domain(value, &command_type)?,
        ScenarioCommand::ApplyDomainCommand(value) => return Ok(value),
        ScenarioCommand::ApplyBatch(_) => {
            return Err(CommandError::Unsupported {
                pack_id: OFFICIAL_TEST_PACK_ID.to_owned(),
                command_type,
            });
        }
    };
    Ok(DomainCommandEnvelope {
        command_type,
        payload,
    })
}

fn encode_domain<T: Serialize>(value: T, command_type: &str) -> Result<Value, CommandError> {
    serde_json::to_value(value).map_err(|error| CommandError::InvalidDomainPayload {
        command_type: command_type.to_owned(),
        message: error.to_string(),
    })
}

fn effect(
    kind: ChangeKind,
    path: String,
    before: Option<Value>,
    after: Option<Value>,
    inverse: ScenarioCommand,
    summary: String,
    command_type: &str,
) -> PackCommandEffect {
    PackCommandEffect {
        changes: vec![Change {
            kind,
            path,
            before,
            after,
        }],
        inverse,
        summary,
        command_type: command_type.to_owned(),
    }
}
/// Validate the structural invariants required by the official Phase-01 command seam.
///
/// This is used by import staging as well as command application so malformed
/// records are rejected before they can reach persistence.
///
/// # Errors
///
/// Returns [`CommandError::Validation`] when a domain record is not an object
/// or an embedded record identity differs from its map key.
pub fn validate_document_shape(document: &ScenarioDocument) -> Result<(), CommandError> {
    validate_map(&document.domain.entities, "/domain/entities")?;
    validate_map(&document.domain.rules, "/domain/rules")?;
    validate_map(&document.domain.preferences, "/domain/preferences")?;
    validate_map(
        &document.domain.locked_assignments,
        "/domain/lockedAssignments",
    )?;
    validate_safe_serialized(document, "/document")?;
    Ok(())
}

const COMMAND_JSON_LIMITS: PortableJsonLimits = PortableJsonLimits {
    max_depth: 128,
    max_string_bytes: 1024 * 1024,
    max_collection_items: 1_000_000,
};

fn validate_safe_serialized<T: Serialize>(value: &T, path: &str) -> Result<(), CommandError> {
    let serialized = serde_json::to_value(value).map_err(|error| {
        validation_error(
            CODE_PROHIBITED_DATA,
            path,
            format!("value cannot be checked before application: {error}"),
        )
    })?;
    validate_nonsecret_portable_json(&serialized, &COMMAND_JSON_LIMITS)
        .map_err(|error| validation_error(CODE_PROHIBITED_DATA, path, error.to_string()))
}

fn validate_map<K>(values: &BTreeMap<K, Value>, base: &str) -> Result<(), CommandError>
where
    K: Ord + std::fmt::Display,
{
    for (map_id, value) in values {
        let Some(object) = value.as_object() else {
            return Err(validation_error(
                CODE_INVALID_RECORD_SHAPE,
                format!("{base}/{map_id}"),
                "record must be a JSON object",
            ));
        };
        if let Some(id_value) = object.get("id") {
            let expected_id = map_id.to_string();
            if id_value.as_str() != Some(expected_id.as_str()) {
                return Err(validation_error(
                    CODE_RECORD_ID_MISMATCH,
                    format!("{base}/{map_id}/id"),
                    format!("record id must match map key {expected_id}"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_record<I: std::fmt::Display>(
    value: &Value,
    expected_id: &I,
    path: &str,
) -> Result<(), CommandError> {
    let Some(object) = value.as_object() else {
        return Err(validation_error(
            CODE_INVALID_RECORD_SHAPE,
            path,
            "record must be a JSON object",
        ));
    };
    if let Some(id_value) = object.get("id") {
        let Some(id) = id_value.as_str() else {
            return Err(validation_error(
                CODE_RECORD_ID_MISMATCH,
                format!("{path}/id"),
                "record id must be a string when present",
            ));
        };
        let expected_id = expected_id.to_string();
        if id != expected_id.as_str() {
            return Err(validation_error(
                CODE_RECORD_ID_MISMATCH,
                format!("{path}/id"),
                format!("record id {id} does not match map key {expected_id}"),
            ));
        }
    }
    validate_safe_serialized(value, path)?;
    Ok(())
}

fn validation_delta(before: &[ValidationIssue], after: &[ValidationIssue]) -> ValidationDelta {
    let before_by_key: BTreeMap<String, &ValidationIssue> = before
        .iter()
        .map(|issue| (validation_issue_key(issue), issue))
        .collect();
    let after_by_key: BTreeMap<String, &ValidationIssue> = after
        .iter()
        .map(|issue| (validation_issue_key(issue), issue))
        .collect();
    let added = after_by_key
        .iter()
        .filter(|(key, _)| !before_by_key.contains_key(*key))
        .map(|(_, issue)| (*issue).clone())
        .collect();
    let resolved = before
        .iter()
        .map(|issue| issue.code.as_str())
        .filter(|code| !after.iter().any(|issue| issue.code == *code))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(str::to_owned)
        .collect();
    ValidationDelta { added, resolved }
}

fn validation_issue_key(issue: &ValidationIssue) -> String {
    match &issue.field_path {
        Some(path) => format!("{}:{path}", issue.code),
        None => issue.code.clone(),
    }
}

fn validation_error(
    code: &'static str,
    path: impl Into<String>,
    message: impl Into<String>,
) -> CommandError {
    CommandError::Validation {
        code,
        path: path.into(),
        message: message.into(),
    }
}

fn entity_path(id: &PersonId) -> String {
    format!("/domain/entities/{id}")
}

fn rule_path(id: &RuleId) -> String {
    format!("/domain/rules/{id}")
}

fn preference_path(id: &RuleId) -> String {
    format!("/domain/preferences/{id}")
}

fn lock_path(id: &AssignmentId) -> String {
    format!("/domain/lockedAssignments/{id}")
}

/// Return stable command type metadata without applying the command.
#[must_use]
pub fn command_type(command: &ScenarioCommand) -> String {
    match command {
        ScenarioCommand::AddEntity(_) => "add_entity".to_owned(),
        ScenarioCommand::UpdateEntity(_) => "update_entity".to_owned(),
        ScenarioCommand::RemoveEntity(_) => "remove_entity".to_owned(),
        ScenarioCommand::AddRule(_) => "add_rule".to_owned(),
        ScenarioCommand::UpdateRule(_) => "update_rule".to_owned(),
        ScenarioCommand::RemoveRule(_) => "remove_rule".to_owned(),
        ScenarioCommand::SetPreference(_) => "set_preference".to_owned(),
        ScenarioCommand::LockAssignment(_) => "lock_assignment".to_owned(),
        ScenarioCommand::UnlockAssignment(_) => "unlock_assignment".to_owned(),
        ScenarioCommand::ApplyDomainCommand(value) => format!("domain.{}", value.command_type),
        ScenarioCommand::ApplyBatch(_) => "apply_batch".to_owned(),
    }
}

/// Return a deterministic human-readable summary without applying the command.
#[must_use]
pub fn human_summary(command: &ScenarioCommand) -> String {
    match command {
        ScenarioCommand::AddEntity(value) => format!("Add entity {}", value.entity_id),
        ScenarioCommand::UpdateEntity(value) => format!("Update entity {}", value.entity_id),
        ScenarioCommand::RemoveEntity(value) => format!("Remove entity {}", value.entity_id),
        ScenarioCommand::AddRule(value) => format!("Add rule {}", value.rule_id),
        ScenarioCommand::UpdateRule(value) => format!("Update rule {}", value.rule_id),
        ScenarioCommand::RemoveRule(value) => format!("Remove rule {}", value.rule_id),
        ScenarioCommand::SetPreference(value) => {
            let action = if value.value.is_some() {
                "Set"
            } else {
                "Clear"
            };
            format!("{action} preference {}", value.preference_id)
        }
        ScenarioCommand::LockAssignment(value) => {
            format!("Lock assignment {}", value.assignment_id)
        }
        ScenarioCommand::UnlockAssignment(value) => {
            format!("Unlock assignment {}", value.assignment_id)
        }
        ScenarioCommand::ApplyDomainCommand(value) => {
            format!("Apply {} domain command", value.command_type)
        }
        ScenarioCommand::ApplyBatch(value) => {
            let count = value.commands.len();
            match &value.label {
                Some(label) => format!("{label} ({count} commands)"),
                None => format!("Apply batch ({count} commands)"),
            }
        }
    }
}
