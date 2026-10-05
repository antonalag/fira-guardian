//! AC-1, AC-2, AC-5: unit tests for the pure deterministic confidence rubric.
//!
//! These exercise `recompute_confidence_outcome` (the typed rubric) and
//! `recorded_contradicts` (the VR5 compare step) directly, covering every
//! deterministic branch the frozen contract fixes — positive and negative —
//! without going through JSON. Findings are built from JSON for brevity (this
//! also exercises the real deserialization path) and then fed to the typed API.

use serde_json::{json, Value};

use fira_core::confidence::{recompute_confidence_outcome, recorded_contradicts, ConfidenceOutcome};
use fira_core::finding::Finding;
use fira_core::model::Confidence;

/// Build a `Finding` from overrides merged onto a valid base. The base is a
/// publishable, HIGH-worthy, non-failure-class finding; each test overrides only
/// the fields it cares about.
fn finding_with(overrides: Value) -> Finding {
    let mut base = json!({
        "id": "RR-001",
        "maturity": "FINDING",
        "title": "t",
        "severity": "LOW",
        "confidence": "HIGH",
        "confidence_derivation": "d",
        "category": "input-validation",
        "requirement_source": "declared_claim",
        "gates_affected": ["Tests"],
        "epistemic_state": { "facets": ["IMPLEMENTED", "OBSERVED"], "conclusion": "VERIFIED" },
        "support_mappings": [
            { "claim": "c", "evidence_refs": ["E-1"], "relevant_span": "s", "sufficiency": "SUFFICIENT" }
        ],
        "impact": "i",
        "release_impact": "NON_BLOCKING",
        "recommended_remediation": "r",
        "verification_criteria": "v",
        "lifecycle_status": "OPEN",
        "regression": false
    });
    let obj = base.as_object_mut().unwrap();
    for (k, v) in overrides.as_object().unwrap() {
        obj.insert(k.clone(), v.clone());
    }
    serde_json::from_value(base).expect("finding deserializes")
}

#[test]
fn no_sufficient_mapping_is_not_publishable() {
    let f = finding_with(json!({
        "support_mappings": [
            { "claim": "c", "evidence_refs": ["E-1"], "relevant_span": "s",
              "sufficiency": "INSUFFICIENT", "unverified_remainder": "all" }
        ]
    }));
    assert_eq!(recompute_confidence_outcome(&f), ConfidenceOutcome::NotPublishable);
}

#[test]
fn sufficient_with_observed_nonfailure_is_high() {
    let f = finding_with(json!({})); // base: OBSERVED facet, non-failure category
    assert_eq!(recompute_confidence_outcome(&f), ConfidenceOutcome::High);
}

#[test]
fn sufficient_without_observed_is_publishable_non_high() {
    let f = finding_with(json!({
        "epistemic_state": { "facets": ["SPECIFIED", "IMPLEMENTED"], "conclusion": "UNVERIFIED" }
    }));
    assert_eq!(
        recompute_confidence_outcome(&f),
        ConfidenceOutcome::PublishableNonHigh
    );
}

#[test]
fn failure_class_high_requires_failure_scenario() {
    // Failure-class via category "recovery", OBSERVED present, but NO
    // failure_scenario -> cannot be HIGH (E5), so PublishableNonHigh.
    let f = finding_with(json!({ "category": "recovery" }));
    assert_eq!(
        recompute_confidence_outcome(&f),
        ConfidenceOutcome::PublishableNonHigh
    );
}

#[test]
fn failure_class_with_scenario_is_high() {
    let f = finding_with(json!({
        "category": "recovery",
        "failure_scenario": { "anchor": "src/x.rs:10-20", "trigger": "t", "consequence": "c" }
    }));
    assert_eq!(recompute_confidence_outcome(&f), ConfidenceOutcome::High);
}

#[test]
fn failure_class_detected_via_gate_too() {
    // Failure class via a gate name rather than the category token.
    let f = finding_with(json!({
        "category": "general",
        "gates_affected": ["Concurrency"]
    }));
    // OBSERVED present but no failure_scenario -> not HIGH.
    assert_eq!(
        recompute_confidence_outcome(&f),
        ConfidenceOutcome::PublishableNonHigh
    );
}

#[test]
fn rubric_is_pure_and_deterministic() {
    let f = finding_with(json!({}));
    let a = recompute_confidence_outcome(&f);
    let b = recompute_confidence_outcome(&f);
    assert_eq!(a, b);
}

// --- recorded_contradicts: the VR5 compare matrix (every determinable aspect) ---

#[test]
fn not_publishable_contradicts_every_recorded_level() {
    for recorded in [Confidence::High, Confidence::Medium, Confidence::Low] {
        assert!(
            recorded_contradicts(recorded, ConfidenceOutcome::NotPublishable),
            "NotPublishable must contradict recorded {recorded:?}"
        );
    }
}

#[test]
fn publishable_non_high_contradicts_only_recorded_high() {
    assert!(recorded_contradicts(Confidence::High, ConfidenceOutcome::PublishableNonHigh));
    assert!(!recorded_contradicts(Confidence::Medium, ConfidenceOutcome::PublishableNonHigh));
    assert!(!recorded_contradicts(Confidence::Low, ConfidenceOutcome::PublishableNonHigh));
}

#[test]
fn high_outcome_contradicts_nothing_downward_resolution() {
    // Downward resolution (§8/CONF-1): a lower recorded value is defensible.
    assert!(!recorded_contradicts(Confidence::High, ConfidenceOutcome::High));
    assert!(!recorded_contradicts(Confidence::Medium, ConfidenceOutcome::High));
    assert!(!recorded_contradicts(Confidence::Low, ConfidenceOutcome::High));
}
