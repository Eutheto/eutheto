//! Shared portable-data safety, identity, asset, and reference contracts.

use crate::{Revision, ScenarioId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;
use uuid::Uuid;

/// Limits applied by the shared recursive JSON safety policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortableJsonLimits {
    pub max_depth: usize,
    pub max_string_bytes: usize,
    pub max_collection_items: usize,
}

/// A nonsecret/nonportable JSON policy violation safe to surface to callers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortableJsonViolation(pub String);

impl fmt::Display for PortableJsonViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PortableJsonViolation {}

/// Applies the one recursive nonsecret/nonportable JSON policy shared by
/// command application, import, and export.
///
/// Credential-bearing field matching tokenizes camel, snake, kebab, and
/// namespaced boundaries, while an explicit narrow allowlist preserves known
/// nonsecret identifiers without generic substring matching.
///
/// # Errors
///
/// Returns a violation for excessive nesting/size, credential-bearing fields,
/// host paths and database/device locations, or executable/template content.
pub fn validate_nonsecret_portable_json(
    value: &Value,
    limits: &PortableJsonLimits,
) -> Result<(), PortableJsonViolation> {
    validate_json_at(value, limits, 0)
}

fn validate_json_at(
    value: &Value,
    limits: &PortableJsonLimits,
    depth: usize,
) -> Result<(), PortableJsonViolation> {
    if depth > limits.max_depth {
        return Err(PortableJsonViolation(
            "JSON nesting limit exceeded".to_owned(),
        ));
    }
    match value {
        Value::String(text) => validate_string(text, limits),
        Value::Array(values) => {
            if values.len() > limits.max_collection_items {
                return Err(PortableJsonViolation(
                    "collection limit exceeded".to_owned(),
                ));
            }
            for item in values {
                validate_json_at(item, limits, depth.saturating_add(1))?;
            }
            Ok(())
        }
        Value::Object(values) => {
            if values.len() > limits.max_collection_items {
                return Err(PortableJsonViolation("object limit exceeded".to_owned()));
            }
            for (key, item) in values {
                if key.len() > limits.max_string_bytes {
                    return Err(PortableJsonViolation(
                        "object key limit exceeded".to_owned(),
                    ));
                }
                let normalized = normalize_field(key);
                if is_prohibited_field(key) {
                    return Err(PortableJsonViolation(format!(
                        "prohibited field {key} cannot be serialized"
                    )));
                }
                if normalized == "redistributionpermitted" && item == &Value::Bool(false) {
                    return Err(PortableJsonViolation(
                        "provider-restricted content cannot be serialized".to_owned(),
                    ));
                }
                validate_json_at(item, limits, depth.saturating_add(1))?;
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

fn validate_string(text: &str, limits: &PortableJsonLimits) -> Result<(), PortableJsonViolation> {
    if text.len() > limits.max_string_bytes {
        return Err(PortableJsonViolation("string limit exceeded".to_owned()));
    }
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let absolute_path = lower.starts_with('/')
        || lower.starts_with("file://")
        || lower.starts_with("\\\\")
        || (bytes.len() > 2 && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/'));
    let database_path = looks_like_database_path(&lower);
    let device_path = is_windows_device_name(&lower) || lower.starts_with("device://");
    if absolute_path || database_path || device_path {
        return Err(PortableJsonViolation(
            "device-specific, database, or local filesystem path is not portable".to_owned(),
        ));
    }
    let template = (lower.contains("{{") && lower.contains("}}"))
        || (lower.contains("{%") && lower.contains("%}"))
        || (lower.contains("<%") && lower.contains("%>"));
    let active = lower.starts_with("#!")
        || lower.contains("<script")
        || lower.contains("javascript:")
        || lower.contains("<svg");
    if template || active || looks_like_private_key_pem(&lower) || looks_like_auth_material(&lower)
    {
        return Err(PortableJsonViolation(
            "executable, credential, or template-like content is not portable".to_owned(),
        ));
    }
    Ok(())
}

fn looks_like_private_key_pem(text: &str) -> bool {
    text.lines().any(|line| {
        let Some(marker) = line.trim().strip_prefix("-----begin ") else {
            return false;
        };
        let Some(label) = marker.strip_suffix("-----") else {
            return false;
        };
        label.trim_end().ends_with("private key")
    })
}

fn looks_like_auth_material(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        let credential = line
            .strip_prefix("authorization:")
            .or_else(|| line.strip_prefix("proxy-authorization:"))
            .map_or(line, str::trim_start);
        ["bearer ", "basic ", "digest ", "aws4-hmac-sha256 "]
            .iter()
            .any(|scheme| {
                credential
                    .strip_prefix(scheme)
                    .is_some_and(|value| !value.trim().is_empty())
            })
            || line.starts_with("cookie:")
            || line.starts_with("set-cookie:")
            || line.starts_with("x-api-key:")
    })
}

fn normalize_field(field: &str) -> String {
    field
        .chars()
        .filter(|character| !matches!(character, '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_prohibited_field(field: &str) -> bool {
    let normalized = normalize_field(field);
    if matches!(
        normalized.as_str(),
        "secretary"
            | "tokenizedlabel"
            | "passwordpolicy"
            | "passwordpolicyname"
            | "vaultedceiling"
            | "externalcredentiallabel"
            | "authenticationstatus"
            | "authenticationlabel"
            | "authenticationmethod"
    ) {
        return false;
    }
    let tokens = field_tokens(field);
    if tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "auth"
                | "authentication"
                | "authenticator"
                | "authorization"
                | "bearer"
                | "secret"
                | "secrets"
                | "token"
                | "tokens"
                | "credential"
                | "credentials"
                | "password"
                | "passphrase"
                | "cookie"
                | "keychain"
                | "vault"
                | "keystore"
                | "dsn"
                | "template"
                | "script"
                | "executable"
        )
    }) {
        return true;
    }
    if tokens.windows(2).any(|pair| {
        matches!(
            (pair[0].as_str(), pair[1].as_str()),
            (
                "api" | "access" | "private" | "signing" | "encryption",
                "key"
            ) | ("database", "path" | "file")
                | ("sqlite", "database" | "path")
                | ("connection", "string")
                | ("device", "path")
                | ("command", "line")
        )
    }) {
        return true;
    }
    compact_prohibited_field(&normalized)
}

fn field_tokens(field: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut previous = None;
    let mut characters = field.chars().peekable();
    while let Some(character) = characters.next() {
        if !character.is_ascii_alphanumeric() {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
            previous = None;
            continue;
        }
        let boundary = !token.is_empty()
            && character.is_ascii_uppercase()
            && previous.is_some_and(|previous: char| {
                previous.is_ascii_lowercase()
                    || previous.is_ascii_digit()
                    || (previous.is_ascii_uppercase()
                        && characters.peek().is_some_and(char::is_ascii_lowercase))
            });
        if boundary {
            tokens.push(std::mem::take(&mut token));
        }
        token.push(character.to_ascii_lowercase());
        previous = Some(character);
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

fn compact_prohibited_field(normalized: &str) -> bool {
    const BASES: [&str; 36] = [
        "auth",
        "authentication",
        "authenticator",
        "authorization",
        "authorizationheader",
        "bearer",
        "secret",
        "secrets",
        "token",
        "tokens",
        "credential",
        "credentials",
        "password",
        "passphrase",
        "apikey",
        "accesskey",
        "clientsecret",
        "privatekey",
        "privatekeypem",
        "secretkey",
        "signingkey",
        "encryptionkey",
        "cookie",
        "sessioncookie",
        "keychain",
        "vault",
        "keystore",
        "connectionstring",
        "dsn",
        "databasepath",
        "databasefile",
        "sqlitedatabase",
        "sqlitepath",
        "devicepath",
        "executablepath",
        "commandline",
    ];
    const SUFFIXES: [&str; 6] = ["id", "raw", "ref", "reference", "handle", "item"];
    if BASES.iter().any(|base| normalized.ends_with(base)) {
        return true;
    }
    SUFFIXES.iter().any(|suffix| {
        normalized
            .strip_suffix(suffix)
            .is_some_and(|stem| BASES.iter().any(|base| stem.ends_with(base)))
    })
}

fn looks_like_database_path(text: &str) -> bool {
    const DATABASE_SUFFIXES: [&[u8]; 4] = [b".db", b".db3", b".sqlite", b".sqlite3"];
    let candidate = text.split(['?', '#']).next().unwrap_or(text).as_bytes();
    DATABASE_SUFFIXES
        .iter()
        .any(|&suffix| candidate.strip_suffix(suffix).is_some())
}

fn is_windows_device_name(text: &str) -> bool {
    let leaf = text.rsplit(['/', '\\']).next().unwrap_or(text);
    let stem = leaf.split('.').next().unwrap_or(leaf);
    matches!(stem, "con" | "prn" | "aux" | "nul")
        || stem
            .strip_prefix("com")
            .or_else(|| stem.strip_prefix("lpt"))
            .is_some_and(|number| {
                matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

/// Supplemental portable section namespace.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupplementalSectionKind {
    Results,
    SharedRecords,
    Preferences,
    Assets,
}

impl SupplementalSectionKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Results => "results",
            Self::SharedRecords => "shared_records",
            Self::Preferences => "preferences",
            Self::Assets => "assets",
        }
    }
}

impl fmt::Display for SupplementalSectionKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Stable identity of one supplemental portable record.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupplementalIdentity {
    pub section: SupplementalSectionKind,
    pub key: String,
}

/// Version-1 threshold for the explicit "exclude large assets" backup option.
///
/// This stays below the smallest permitted asset payload ceiling (the
/// one-mebibyte plain-text string limit), so every supported asset kind has a
/// nonempty valid range that the option can exclude.
pub const PORTABLE_LARGE_ASSET_BYTES_V1: usize = 512 * 1024;
/// Inert portable asset plus the exact declaration that authorizes re-export.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableAsset {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub redistribution_permitted: bool,
}

/// One retained-result dependency on an exact scenario revision.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ScenarioRevisionReference {
    pub scenario_id: ScenarioId,
    pub scenario_revision: Revision,
}

/// Error produced by declared scenario-reference extraction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortableReferenceError(pub String);

impl fmt::Display for PortableReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PortableReferenceError {}

/// Extracts UUIDs from exact `scenarioId`/`scenarioIds` positions recursively.
/// Arbitrary UUID-shaped prose and external values are ignored.
///
/// # Errors
///
/// Returns an error when a declared scenario-reference position is malformed.
pub fn extract_scenario_references(
    value: &Value,
) -> Result<BTreeSet<ScenarioId>, PortableReferenceError> {
    let mut references = BTreeSet::new();
    collect_scenario_references(value, &mut references)?;
    Ok(references)
}

fn collect_scenario_references(
    value: &Value,
    references: &mut BTreeSet<ScenarioId>,
) -> Result<(), PortableReferenceError> {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_scenario_references(value, references)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                match normalize_field(key).as_str() {
                    "scenarioid" => {
                        references.insert(parse_scenario_id(value, key)?);
                    }
                    "scenarioids" => {
                        let ids = value.as_array().ok_or_else(|| {
                            PortableReferenceError(format!("declared field {key} must be an array"))
                        })?;
                        for id in ids {
                            references.insert(parse_scenario_id(id, key)?);
                        }
                    }
                    _ => collect_scenario_references(value, references)?,
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

/// Extracts exact declared inert-asset references without inspecting prose.
///
/// # Errors
///
/// Returns an error when a declared scalar/list asset-reference field is malformed.
pub fn extract_asset_references(value: &Value) -> Result<BTreeSet<String>, PortableReferenceError> {
    let mut references = BTreeSet::new();
    collect_asset_references(value, &mut references)?;
    Ok(references)
}

fn collect_asset_references(
    value: &Value,
    references: &mut BTreeSet<String>,
) -> Result<(), PortableReferenceError> {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_asset_references(value, references)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                let normalized = normalize_field(key);
                if matches!(
                    normalized.as_str(),
                    "asset" | "assetid" | "assetkey" | "assetpath"
                ) {
                    let reference = value.as_str().ok_or_else(|| {
                        PortableReferenceError(format!(
                            "declared asset reference {key} must be a string"
                        ))
                    })?;
                    references.insert(reference.to_owned());
                } else if matches!(
                    normalized.as_str(),
                    "assets" | "assetids" | "assetkeys" | "assetpaths"
                ) {
                    let items = value.as_array().ok_or_else(|| {
                        PortableReferenceError(format!(
                            "declared asset reference list {key} must be an array"
                        ))
                    })?;
                    for item in items {
                        let reference = item.as_str().ok_or_else(|| {
                            PortableReferenceError(format!(
                                "declared asset reference list {key} must contain strings"
                            ))
                        })?;
                        references.insert(reference.to_owned());
                    }
                } else {
                    collect_asset_references(value, references)?;
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn parse_scenario_id(value: &Value, field: &str) -> Result<ScenarioId, PortableReferenceError> {
    let text = value.as_str().ok_or_else(|| {
        PortableReferenceError(format!("declared field {field} must be a UUID string"))
    })?;
    let id = Uuid::parse_str(text).map_err(|_| {
        PortableReferenceError(format!("declared field {field} must be a UUID string"))
    })?;
    if id.get_version_num() != 7 {
        return Err(PortableReferenceError(format!(
            "declared field {field} must be a UUIDv7 string"
        )));
    }
    Ok(ScenarioId::from_uuid(id))
}

/// Extracts the required UUID `resultId` from a retained-result wrapper.
///
/// # Errors
///
/// Returns an error when the wrapper is not an object or `resultId` is
/// missing, ambiguous, malformed, or not a `UUIDv7`.
pub fn extract_result_id(value: &Value) -> Result<Uuid, PortableReferenceError> {
    let object = value.as_object().ok_or_else(|| {
        PortableReferenceError("retained result must be a JSON object".to_owned())
    })?;
    let (key, value) = unique_normalized_field(object, "resultid", "resultId")?;
    let text = value
        .as_str()
        .ok_or_else(|| PortableReferenceError(format!("retained result {key} must be a string")))?;
    let id = Uuid::parse_str(text)
        .map_err(|_| PortableReferenceError(format!("retained result {key} must be a UUID")))?;
    if id.get_version_num() != 7 {
        return Err(PortableReferenceError(format!(
            "retained result {key} must be a UUIDv7"
        )));
    }
    Ok(id)
}

/// Extracts the required exact `(scenarioId, scenarioRevision)` dependency from
/// a retained-result wrapper. Result-domain payload remains otherwise opaque.
///
/// # Errors
///
/// Returns an error when the wrapper is not an object or either required field
/// is missing or malformed.
pub fn extract_result_dependency(
    value: &Value,
) -> Result<ScenarioRevisionReference, PortableReferenceError> {
    let object = value.as_object().ok_or_else(|| {
        PortableReferenceError("retained result must be a JSON object".to_owned())
    })?;
    let (scenario_key, scenario_value) =
        unique_normalized_field(object, "scenarioid", "scenarioId")?;
    let scenario_id = parse_scenario_id(scenario_value, scenario_key)?;
    let (_, revision_value) =
        unique_normalized_field(object, "scenariorevision", "scenarioRevision")?;
    let revision = revision_value.as_u64().ok_or_else(|| {
        PortableReferenceError("retained result scenarioRevision must be an integer".to_owned())
    })?;
    Ok(ScenarioRevisionReference {
        scenario_id,
        scenario_revision: Revision::new(revision),
    })
}

fn unique_normalized_field<'a>(
    object: &'a serde_json::Map<String, Value>,
    normalized: &str,
    display: &str,
) -> Result<(&'a str, &'a Value), PortableReferenceError> {
    let mut matches = object
        .iter()
        .filter(|(key, _)| normalize_field(key) == normalized);
    let (key, value) = matches
        .next()
        .ok_or_else(|| PortableReferenceError(format!("retained result is missing {display}")))?;
    if matches.next().is_some() {
        return Err(PortableReferenceError(format!(
            "retained result contains ambiguous {display} fields"
        )));
    }
    Ok((key.as_str(), value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const LIMITS: PortableJsonLimits = PortableJsonLimits {
        max_depth: 16,
        max_string_bytes: 1024,
        max_collection_items: 1024,
    };

    #[test]
    fn exact_credential_fields_are_rejected_without_substring_matching()
    -> Result<(), PortableJsonViolation> {
        for key in [
            "secret",
            "raw_secret",
            "clientSecret",
            "private_key_pem",
            "authorization-header",
            "credentialRef",
            "keychain_ref",
            "vaultReference",
            "token-reference",
            "providerApiKey",
            "providerClientSecret",
            "oauthCredentialId",
            "apiKeyId",
            "provider-api-key-reference",
            "providerSecretHandle",
            "oauth_token_item",
            "api_key_ref",
            "passwordRef",
            "auth",
            "authenticationRef",
            "authenticatorHandle",
            "authorizationHeader",
            "bearer",
            "connectionString",
            "providerDsn",
            "providerAuthentication.value",
        ] {
            let mut object = serde_json::Map::new();
            object.insert(key.to_owned(), Value::String("sentinel".to_owned()));
            let value = Value::Object(object);
            assert!(validate_nonsecret_portable_json(&value, &LIMITS).is_err());
        }
        validate_nonsecret_portable_json(
            &json!({
                "secretary": "safe",
                "tokenizedLabel": "safe",
                "passwordPolicyName": "safe",
                "vaultedCeiling": "safe",
                "externalCredentialLabel": "safe",
                "authenticationStatus": "connected",
                "authenticationLabel": "Provider",
                "authenticationMethod": "browser",
                "grid": "safe",
                "tokenizer": "safe",
                "monkey": "safe"
            }),
            &LIMITS,
        )?;
        for value in [
            json!({"providerAuthentication": {"value": "sentinel"}}),
            json!({"auth": {"bearer": "sentinel"}}),
        ] {
            assert!(validate_nonsecret_portable_json(&value, &LIMITS).is_err());
        }
        Ok(())
    }

    #[test]
    fn credential_shaped_values_are_rejected_independently_of_field_names()
    -> Result<(), PortableJsonViolation> {
        for value in [
            json!({"headers": ["Authorization: Bearer synthetic-bearer-sentinel"]}),
            json!({"headers": ["Proxy-Authorization: Basic c3ludGhldGljOnNlbnRpbmVs"]}),
            json!({"headers": ["Digest username=\"synthetic\", response=\"sentinel\""]}),
            json!({"headers": ["AWS4-HMAC-SHA256 Credential=synthetic/sentinel"]}),
            json!({"headers": ["Cookie: session=synthetic-sentinel"]}),
            json!({"headers": ["Set-Cookie: session=synthetic-sentinel"]}),
            json!({"headers": ["X-API-Key: synthetic-sentinel"]}),
            json!({"pem": "-----BEGIN RSA PRIVATE KEY-----\nsynthetic\n-----END RSA PRIVATE KEY-----"}),
            json!({"pem": "-----BEGIN EC PRIVATE KEY-----\nsynthetic\n-----END EC PRIVATE KEY-----"}),
            json!({"pem": "-----BEGIN OPENSSH PRIVATE KEY-----\nsynthetic\n-----END OPENSSH PRIVATE KEY-----"}),
            json!({"pem": "-----BEGIN ENCRYPTED PRIVATE KEY-----\nsynthetic\n-----END ENCRYPTED PRIVATE KEY-----"}),
        ] {
            assert!(validate_nonsecret_portable_json(&value, &LIMITS).is_err());
        }
        validate_nonsecret_portable_json(
            &json!({
                "note": "The bearer of this certificate may proceed.",
                "certificate": "-----BEGIN CERTIFICATE-----\nsynthetic\n-----END CERTIFICATE-----"
            }),
            &LIMITS,
        )
    }

    #[test]
    fn retained_result_identity_is_required_uuid_v7() -> Result<(), PortableReferenceError> {
        let expected = Uuid::parse_str("018f1e2d-3c4b-7a69-8def-0123456789ab")
            .map_err(|error| PortableReferenceError(error.to_string()))?;
        assert_eq!(extract_result_id(&json!({"resultId": expected}))?, expected);
        assert!(extract_result_id(&json!({})).is_err());
        assert!(extract_result_id(&json!({"resultId": Uuid::nil()})).is_err());
        Ok(())
    }

    #[test]
    fn references_ignore_uuid_shaped_prose() -> Result<(), PortableReferenceError> {
        let owned = "018f1e2d-3c4b-7a69-8def-0123456789ab";
        let external = "018f1e2d-3c4b-7a69-8def-1123456789ab";
        let references = extract_scenario_references(&json!({
            "scenarioId": owned,
            "note": external,
            "externalUuid": external
        }))?;
        assert_eq!(references.len(), 1);
        assert_eq!(
            references.iter().next().map(ToString::to_string),
            Some(owned.to_owned())
        );
        Ok(())
    }
}
