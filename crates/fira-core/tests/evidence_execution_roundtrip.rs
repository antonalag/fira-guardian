//! AC-3: `serde` round-trip of the finalized S4/S5/S12 types against the Task 2
//! valid example corpus.
//!
//! Each fixture deserializes into its Rust type and re-serializes to a
//! `serde_json::Value` equal to the input (object key order is irrelevant for
//! `Value` equality). `#[serde(deny_unknown_fields)]` plus `skip_serializing_if`
//! make this a strict shape-parity check.

mod common;

use common::load_json;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use fira_core::evidence::{EvidenceRef, SupportMapping};
use fira_core::execution::{ExecutionResult, VerificationMechanism};

fn assert_roundtrip<T: DeserializeOwned + Serialize>(fixture: &str) {
    let original = load_json(&format!("schemas/examples/valid/{fixture}"));
    let typed: T = serde_json::from_value(original.clone())
        .unwrap_or_else(|e| panic!("{fixture}: deserialize into type failed: {e}"));
    let reserialized: Value = serde_json::to_value(&typed)
        .unwrap_or_else(|e| panic!("{fixture}: serialize back failed: {e}"));
    assert_eq!(
        reserialized, original,
        "{fixture}: round-trip changed the JSON (left=reserialized, right=original)"
    );
}

#[test]
fn s04_evidence_ref_roundtrips() {
    assert_roundtrip::<EvidenceRef>("s04-evidence-ref.json");
}

#[test]
fn s04_support_mapping_roundtrips() {
    assert_roundtrip::<SupportMapping>("s04-support-mapping.json");
}

#[test]
fn s05_execution_result_roundtrips() {
    assert_roundtrip::<ExecutionResult>("s05-execution-result.json");
}

#[test]
fn s12_verification_mechanism_roundtrips() {
    assert_roundtrip::<VerificationMechanism>("s12-verification-mechanism.json");
}
