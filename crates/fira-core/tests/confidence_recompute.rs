//! AC-3, AC-4: VR5 recompute-and-compare over report values via the
//! `ConfidenceRubric` hook.
//!
//! Positive case: the golden report passes (no VR5 violations). Negative cases:
//! a published finding with no SUFFICIENT mapping (the Task 2 VR5-surface
//! fixture), an overclaimed HIGH (no execution-based facet), and a failure-class
//! HIGH with no exercised failure scenario — each must raise a VR5 violation.
//! A recorded MEDIUM/LOW that the model cannot deterministically distinguish must
//! NOT be flagged.

mod common;

use common::load_json;
use serde_json::{json, Value};

use fira_core::confidence::ConfidenceRubric;
use fira_core::validation::{RecomputeHook, Vr};

fn vr5_violations(report: &Value) -> usize {
    let hook = ConfidenceRubric;
    hook.recompute_confidence(report)
        .iter()
        .filter(|v| v.vr == Vr::Vr5)
        .count()
}

/// AC-4 (positive): the golden composite report has no VR5 violations.
#[test]
fn golden_report_passes_vr5() {
    let report = load_json("schemas/examples/valid/s11-audit-report.json");
    let hook = ConfidenceRubric;
    let violations = hook.recompute_confidence(&report);
    assert!(
        violations.is_empty(),
        "golden report must pass VR5 recompute, got: {violations:?}"
    );
}

/// AC-4 (negative): the Task 2 VR5-surface fixture — a published finding with no
/// SUFFICIENT mapping — is rejected by VR5 (CONF-2/VR1 overlap).
#[test]
fn insufficient_finding_rejected_by_vr5() {
    let report = load_json("schemas/examples/invalid/vr05-surface-insufficient-finding.json");
    assert!(
        vr5_violations(&report) >= 1,
        "a published finding with no SUFFICIENT mapping must raise VR5"
    );
}

/// Load the golden report and replace its single finding's fields with
/// `overrides`, returning the mutated report value.
fn golden_with_finding_overrides(overrides: Value) -> Value {
    let mut report = load_json("schemas/examples/valid/s11-audit-report.json");
    let finding = report["findings"][0].as_object_mut().unwrap();
    for (k, v) in overrides.as_object().unwrap() {
        finding.insert(k.clone(), v.clone());
    }
    report
}

/// Negative: recorded HIGH but no execution-based facet -> overclaimed HIGH.
#[test]
fn overclaimed_high_without_execution_facet_rejected() {
    let report = golden_with_finding_overrides(json!({
        "confidence": "HIGH",
        "epistemic_state": { "facets": ["SPECIFIED", "IMPLEMENTED"], "conclusion": "UNVERIFIED" }
    }));
    assert!(
        vr5_violations(&report) >= 1,
        "recorded HIGH without an execution-based facet must raise VR5"
    );
}

/// Negative: failure-class finding recorded HIGH with an OBSERVED facet but no
/// exercised failure scenario -> fails the E5 HIGH bar.
#[test]
fn failure_class_high_without_scenario_rejected() {
    let report = golden_with_finding_overrides(json!({
        "confidence": "HIGH",
        "category": "recovery",
        "epistemic_state": { "facets": ["OBSERVED"], "conclusion": "VERIFIED" }
        // no failure_scenario present
    }));
    assert!(
        vr5_violations(&report) >= 1,
        "failure-class HIGH without an exercised failure scenario must raise VR5"
    );
}

/// Not flagged: the same evidence that supports HIGH, recorded as the lower
/// MEDIUM/LOW, is defensible (downward resolution) and must NOT raise VR5.
#[test]
fn downward_recorded_confidence_not_flagged() {
    for level in ["MEDIUM", "LOW"] {
        let report = golden_with_finding_overrides(json!({ "confidence": level }));
        assert_eq!(
            vr5_violations(&report),
            0,
            "recorded {level} against HIGH-worthy evidence must not raise VR5 (downward resolution)"
        );
    }
}

/// Not flagged: a publishable-non-HIGH finding recorded as MEDIUM or LOW is fine;
/// only a recorded HIGH would contradict it.
#[test]
fn publishable_non_high_recorded_low_not_flagged() {
    let report = golden_with_finding_overrides(json!({
        "confidence": "LOW",
        "epistemic_state": { "facets": ["SPECIFIED", "IMPLEMENTED"], "conclusion": "UNVERIFIED" }
    }));
    assert_eq!(
        vr5_violations(&report),
        0,
        "publishable-non-HIGH recorded as LOW must not raise VR5"
    );
}
