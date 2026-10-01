//! AC-2, AC-9: field parity between the CORE domain types and the Task 2 JSON
//! Schemas.
//!
//! For each Task-4-owned schema, this asserts that the set of fields the Rust
//! type (de)serializes equals the schema's declared `properties` set — no extra,
//! no missing. The check feeds a **complete** instance (every optional field
//! present) through `T` and compares the serialized object's keys against the
//! schema's property keys. `#[serde(deny_unknown_fields)]` guarantees the type
//! has no field the schema lacks; this test guarantees the type has no *missing*
//! field and the schema has no field the type drops.
//!
//! AC-9 (exact S1–S12 / VR1–VR13 coverage) is already asserted by the Task 2
//! `schema_parity.rs` tests, which remain green; this file adds the typed-model
//! parity for the Task-4-owned subset.

mod common;

use std::collections::BTreeSet;

use common::load_json;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use fira_core::assessment::TechnicalAssessment;
use fira_core::coverage::CoverageStatement;
use fira_core::epistemic::EpistemicState;
use fira_core::finding::Finding;
use fira_core::gate::Gate;
use fira_core::report::AuditReport;
use fira_core::request::AuditRequest;
use fira_core::requirement::DeclaredClaim;

/// Keys of a schema's top-level `properties`.
fn schema_property_keys(schema_file: &str) -> BTreeSet<String> {
    let schema = load_json(&format!("schemas/{schema_file}"));
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

/// Keys produced when a *complete* instance (all optional fields present) is
/// serialized through `T`.
fn type_serialized_keys<T: DeserializeOwned + Serialize>(complete: Value) -> BTreeSet<String> {
    let typed: T = serde_json::from_value(complete)
        .unwrap_or_else(|e| panic!("complete instance did not deserialize: {e}"));
    let value = serde_json::to_value(&typed).expect("serialize");
    value
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect()
}

fn assert_parity<T: DeserializeOwned + Serialize>(schema_file: &str, complete: Value) {
    let schema_keys = schema_property_keys(schema_file);
    let type_keys = type_serialized_keys::<T>(complete);
    assert_eq!(
        type_keys, schema_keys,
        "field parity mismatch for {schema_file}: type={type_keys:?} schema={schema_keys:?}"
    );
}

// Complete instances include EVERY property (incl. optionals) so the serialized
// key set equals the full schema property set.

#[test]
fn s01_parity() {
    let complete = json!({
        "audit_id": "a",
        "project_root": "/p",
        "release_target": "v",
        "execution_boundary": ["READ"],
        "declared_claims": [],
        "audit_profile": "library",
        "prior_audit_report_ref": "/p2",
        "requested_depth": "STANDARD"
    });
    assert_parity::<AuditRequest>("s01-audit-request.schema.json", complete);
}

#[test]
fn s03_parity() {
    let complete = json!({ "facets": ["OBSERVED"], "conclusion": "VERIFIED" });
    assert_parity::<EpistemicState>("s03-epistemic-state.schema.json", complete);
}

#[test]
fn s07_parity() {
    let complete = json!({
        "id": "RR-001",
        "maturity": "FINDING",
        "title": "t",
        "severity": "LOW",
        "confidence": "HIGH",
        "confidence_derivation": "d",
        "category": "c",
        "requirement_source": "declared_claim",
        "security_property": { "source": "declared" },
        "gates_affected": ["Tests"],
        "epistemic_state": { "facets": ["OBSERVED"], "conclusion": "VERIFIED" },
        "support_mappings": [
            { "claim": "c", "evidence_refs": ["E-1"], "relevant_span": "s", "sufficiency": "SUFFICIENT" }
        ],
        "failure_scenario": { "anchor": "f:1", "trigger": "t", "consequence": "c" },
        "impact": "i",
        "release_impact": "NON_BLOCKING",
        "recommended_remediation": "r",
        "verification_criteria": "v",
        "lifecycle_status": "OPEN",
        "regression": false
    });
    assert_parity::<Finding>("s07-finding.schema.json", complete);
}

#[test]
fn s08_parity() {
    let complete = json!({
        "name": "Tests",
        "requirement_level": "required",
        "state": "PASS",
        "rationale": "r",
        "cause": "NONE",
        "support_mappings": [
            { "claim": "c", "evidence_refs": ["E-1"], "relevant_span": "s", "sufficiency": "SUFFICIENT" }
        ],
        "supporting_findings": ["RR-001"]
    });
    assert_parity::<Gate>("s08-gate.schema.json", complete);
}

#[test]
fn s09_parity() {
    let complete = load_json("schemas/examples/valid/s09-coverage-statement.json");
    assert_parity::<CoverageStatement>("s09-coverage-statement.schema.json", complete);
}

#[test]
fn s11_parity() {
    // The golden report includes every S11 property except the optional
    // prior_report_ref; add it so the complete instance covers the full set.
    let mut complete = load_json("schemas/examples/valid/s11-audit-report.json");
    complete["prior_report_ref"] = json!("/repos/other/report.json");
    assert_parity::<AuditReport>("s11-audit-report.schema.json", complete);
}

// S6 DeclaredClaim and S10 TechnicalAssessment live inside $defs-only schemas, so
// their property sets are compared against the relevant $defs entry rather than a
// top-level `properties`.

fn defs_property_keys(schema_file: &str, def: &str) -> BTreeSet<String> {
    let schema = load_json(&format!("schemas/{schema_file}"));
    schema
        .get("$defs")
        .and_then(|d| d.get(def))
        .and_then(|s| s.get("properties"))
        .and_then(Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

#[test]
fn s06_declared_claim_parity() {
    let complete = json!({
        "id": "DC-1", "statement": "s", "source_ref": "r", "security_relevant": false
    });
    let typed: DeclaredClaim = serde_json::from_value(complete).unwrap();
    let keys: BTreeSet<String> = serde_json::to_value(&typed)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        defs_property_keys("s06-requirement-family.schema.json", "DeclaredClaim")
    );
}

#[test]
fn s10_technical_assessment_parity() {
    let complete = json!({
        "result": "READY",
        "blocking_findings": [],
        "blocking_gates": [],
        "risk_findings": [],
        "risk_gates": [],
        "computed_by_rule": "r"
    });
    let typed: TechnicalAssessment = serde_json::from_value(complete).unwrap();
    let keys: BTreeSet<String> = serde_json::to_value(&typed)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        defs_property_keys("s10-assessment-decision.schema.json", "TechnicalAssessment")
    );
}
