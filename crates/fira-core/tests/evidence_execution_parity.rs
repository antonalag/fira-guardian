//! AC-2: field parity between the finalized S4/S5/S12 types and the Task 2 JSON
//! Schemas.
//!
//! For each type, feed a *complete* instance (every optional field present)
//! through `T` and assert the serialized object's key set equals the schema's
//! property set (top-level `properties`, or the relevant `$defs` entry for the
//! `$defs`-only S4 schema). `#[serde(deny_unknown_fields)]` already guarantees
//! the type has no field the schema lacks; this adds the converse.

mod common;

use std::collections::BTreeSet;

use common::load_json;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use fira_core::evidence::{EvidenceRef, SupportMapping};
use fira_core::execution::{ExecutionResult, VerificationMechanism};

fn top_level_keys(schema_file: &str) -> BTreeSet<String> {
    let schema = load_json(&format!("schemas/{schema_file}"));
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

fn defs_keys(schema_file: &str, def: &str) -> BTreeSet<String> {
    let schema = load_json(&format!("schemas/{schema_file}"));
    schema
        .get("$defs")
        .and_then(|d| d.get(def))
        .and_then(|s| s.get("properties"))
        .and_then(Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

fn serialized_keys<T: DeserializeOwned + Serialize>(complete: Value) -> BTreeSet<String> {
    let typed: T = serde_json::from_value(complete)
        .unwrap_or_else(|e| panic!("complete instance did not deserialize: {e}"));
    serde_json::to_value(&typed)
        .expect("serialize")
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn s04_evidence_ref_parity() {
    let complete = json!({
        "ref_id": "E-1",
        "type": "COMMAND",
        "locator": "cmd-1",
        "valid": true,
        "captured_output_ref": "cmd-1"
    });
    assert_eq!(
        serialized_keys::<EvidenceRef>(complete),
        defs_keys("s04-evidence-support-mapping.schema.json", "EvidenceRef")
    );
}

#[test]
fn s04_support_mapping_parity() {
    let complete = json!({
        "claim": "c",
        "evidence_refs": ["E-1"],
        "relevant_span": "s",
        "sufficiency": "INSUFFICIENT",
        "unverified_remainder": "r"
    });
    assert_eq!(
        serialized_keys::<SupportMapping>(complete),
        defs_keys("s04-evidence-support-mapping.schema.json", "SupportMapping")
    );
}

#[test]
fn s05_execution_result_parity() {
    let complete = json!({
        "command_id": "cmd-1",
        "command": "make test",
        "outcome": "PASSED",
        "classification": "APP_LEVEL",
        "blocking_condition": "none",
        "captured_output": "ok",
        "exercised_behaviors": ["b"]
    });
    assert_eq!(
        serialized_keys::<ExecutionResult>(complete),
        top_level_keys("s05-execution-result.schema.json")
    );
}

#[test]
fn s12_verification_mechanism_parity() {
    let complete = json!({
        "id": "cmd-1",
        "source": "MAKE",
        "source_locator": "Makefile:1",
        "command": "make test",
        "declared_by_project": true
    });
    assert_eq!(
        serialized_keys::<VerificationMechanism>(complete),
        top_level_keys("s12-verification-mechanism.schema.json")
    );
}
