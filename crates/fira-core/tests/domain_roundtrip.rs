//! AC-3, AC-4: `serde` round-trip of the CORE domain types against the Task 2
//! valid example corpus.
//!
//! For each Task-4-owned valid fixture, deserialize the JSON into the Rust type
//! and re-serialize; the result must equal the input as a `serde_json::Value`.
//! `serde_json::Value` compares objects independently of key order, so this is a
//! semantic equality check. Because every struct uses
//! `#[serde(deny_unknown_fields)]`, any extra field in the type (or missing
//! required field) would fail deserialization; because optionals use
//! `skip_serializing_if`, omitted inputs re-serialize without the key. Together
//! this proves exact shape parity between the Rust types and the schemas.

mod common;

use common::load_json;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use fira_core::assessment::{HumanDecisionFira, TechnicalAssessment};
use fira_core::coverage::CoverageStatement;
use fira_core::epistemic::EpistemicState;
use fira_core::evidence::SupportMapping;
use fira_core::execution::VerificationMechanism;
use fira_core::finding::Finding;
use fira_core::gate::Gate;
use fira_core::report::AuditReport;
use fira_core::request::AuditRequest;
use fira_core::requirement::DeclaredClaim;

/// Deserialize `fixture` into `T`, re-serialize, and assert equality with the
/// original JSON value.
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
fn s01_audit_request_roundtrips() {
    assert_roundtrip::<AuditRequest>("s01-audit-request.json");
}

#[test]
fn s03_epistemic_state_roundtrips() {
    assert_roundtrip::<EpistemicState>("s03-epistemic-state.json");
}

#[test]
fn s06_declared_claim_roundtrips() {
    assert_roundtrip::<DeclaredClaim>("s06-declared-claim.json");
}

#[test]
fn s07_finding_roundtrips() {
    assert_roundtrip::<Finding>("s07-finding.json");
}

#[test]
fn s08_gate_roundtrips() {
    assert_roundtrip::<Gate>("s08-gate.json");
}

#[test]
fn s09_coverage_statement_roundtrips() {
    assert_roundtrip::<CoverageStatement>("s09-coverage-statement.json");
}

#[test]
fn s10_technical_assessment_roundtrips() {
    assert_roundtrip::<TechnicalAssessment>("s10-technical-assessment.json");
}

#[test]
fn s10_human_decision_fira_roundtrips() {
    assert_roundtrip::<HumanDecisionFira>("s10-human-decision-fira.json");
}

/// AC-4: the golden composite report round-trips through the `AuditReport` type.
#[test]
fn s11_audit_report_golden_roundtrips() {
    assert_roundtrip::<AuditReport>("s11-audit-report.json");
}

// --- Provisional Task-5 seam types: round-trip against their S4/S12 fixtures ---
// These prove the seam types are structurally faithful to the already-defined
// S4/S12 shapes they represent (no Task-5 semantics involved).

#[test]
fn seam_support_mapping_roundtrips() {
    assert_roundtrip::<SupportMapping>("s04-support-mapping.json");
}

#[test]
fn seam_verification_mechanism_roundtrips() {
    assert_roundtrip::<VerificationMechanism>("s12-verification-mechanism.json");
}
