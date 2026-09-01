#![no_main]

#[path = "../support.rs"]
mod support;

use eutheto_planning_ir::{canonical_json, parse_and_validate, validate};
use libfuzzer_sys::fuzz_target;
use serde_json::{Value, json};
use support::LIMITS;

fuzz_target!(|data: &[u8]| {
    if data.len() > LIMITS.max_ir_bytes as usize {
        return;
    }

    let first = parse_and_validate(data, LIMITS);
    let second = parse_and_validate(data, LIMITS);
    assert_eq!(first, second, "planning IR parse/validation must be deterministic");

    let Ok(problem) = first else {
        return;
    };
    assert!(validate(&problem, LIMITS).is_ok());

    let mut canonical = problem.clone();
    canonical
        .canonicalize()
        .expect("validated planning IR must canonicalize");
    assert!(validate(&canonical, LIMITS).is_ok());
    let once = canonical.clone();
    canonical
        .canonicalize()
        .expect("canonical planning IR must canonicalize repeatedly");
    assert_eq!(canonical, once, "planning IR canonicalization must be idempotent");
    assert_eq!(
        canonical_json(&problem, LIMITS).expect("accepted planning IR serializes"),
        canonical_json(&problem, LIMITS).expect("accepted planning IR serializes repeatedly")
    );

    let mut unknown_root = serde_json::to_value(&problem).expect("planning IR serializes as JSON");
    unknown_root
        .as_object_mut()
        .expect("planning IR root is an object")
        .insert(
            "extensions".to_owned(),
            json!({"semantic": true, "payload": data.get(..data.len().min(64)).unwrap_or(data)}),
        );
    let unknown_root = serde_json::to_vec(&unknown_root).expect("attack JSON serializes");
    assert!(
        parse_and_validate(&unknown_root, LIMITS).is_err(),
        "unknown root extensions must be rejected"
    );

    let mut extension_node = serde_json::to_value(&problem).expect("planning IR serializes as JSON");
    extension_node
        .as_object_mut()
        .expect("planning IR root is an object")
        .insert(
            "variables".to_owned(),
            Value::Array(vec![json!({
                "type": "extension",
                "value": {"kind": "recursive", "children": [{"type": "extension"}]}
            })]),
        );
    let extension_node = serde_json::to_vec(&extension_node).expect("attack JSON serializes");
    assert!(
        parse_and_validate(&extension_node, LIMITS).is_err(),
        "unknown or recursive extension nodes must be rejected"
    );
});
