//! AC-3, AC-4: VR7 recompute-and-compare over report values via the
//! `VerdictEngine` hook.
//!
//! Positive: the golden report's recorded assessment matches the recompute (no
//! VR7 violations). Negative: a mismatched `result` and a mismatched
//! blocking/risk set each raise VR7.

mod common;

use common::load_json;
use serde_json::{json, Value};

use fira_core::validation::{RecomputeHook, Vr};
use fira_core::verdict::VerdictEngine;

fn vr7_violations(report: &Value) -> Vec<fira_core::validation::Violation> {
    VerdictEngine
        .recompute_verdict(report)
        .into_iter()
        .filter(|v| v.vr == Vr::Vr7)
        .collect()
}

/// AC-4 (positive): the golden composite report passes VR7 recompute. Its single
/// Tests gate is PASS/required and its one finding is NON_BLOCKING/OPEN, so the
/// recompute is READY with empty sets — matching the recorded assessment.
#[test]
fn golden_report_passes_vr7() {
    let report = load_json("schemas/examples/valid/s11-audit-report.json");
    let violations = VerdictEngine.recompute_verdict(&report);
    assert!(
        violations.is_empty(),
        "golden report must pass VR7 recompute, got: {violations:?}"
    );
}

/// Mutate the golden report's gate state and finding, returning the report.
fn golden_mut() -> Value {
    load_json("schemas/examples/valid/s11-audit-report.json")
}

/// Negative: a required gate is FAIL (recompute ⇒ NOT_READY) but the recorded
/// assessment still says READY ⇒ VR7 on the result (and on blocking_gates).
#[test]
fn mismatched_result_rejected() {
    let mut report = golden_mut();
    // Make the required Tests gate FAIL; leave technical_assessment = READY.
    report["gates"][0]["state"] = json!("FAIL");
    let violations = vr7_violations(&report);
    assert!(
        violations.iter().any(|v| v.locator == "technical_assessment.result"),
        "a required FAIL recorded as READY must raise VR7 on the result; got {violations:?}"
    );
}

/// Negative: recorded blocking/risk sets disagree with the recompute even when
/// the result matches. Here we introduce a recommended-gate UNKNOWN (recompute ⇒
/// READY_WITH_RISKS with risk_gates=[Documentation]) and record READY_WITH_RISKS
/// but with empty sets ⇒ VR7 on the set, not necessarily the result.
#[test]
fn mismatched_sets_rejected() {
    let mut report = golden_mut();
    // Add a recommended gate in UNKNOWN state -> a risk per Step2.
    report["gates"].as_array_mut().unwrap().push(json!({
        "name": "Documentation",
        "requirement_level": "recommended",
        "state": "UNKNOWN",
        "rationale": "not audited",
        "support_mappings": [],
        "supporting_findings": []
    }));
    // Record READY_WITH_RISKS (matching the recomputed result) but with EMPTY
    // risk_gates, so only the set comparison should fire.
    report["technical_assessment"]["result"] = json!("READY_WITH_RISKS");
    report["technical_assessment"]["risk_gates"] = json!([]);

    let violations = vr7_violations(&report);
    assert!(
        violations
            .iter()
            .any(|v| v.locator == "technical_assessment.risk_gates"),
        "a recomputed risk_gates set that differs from the recorded one must raise VR7; got {violations:?}"
    );
}

/// Negative: a CRITICAL+HIGH+BLOCKER finding (recompute ⇒ NOT_READY) recorded as
/// READY ⇒ VR7 on the result and on blocking_findings.
#[test]
fn critical_blocker_recorded_ready_rejected() {
    let mut report = golden_mut();
    let finding = report["findings"][0].as_object_mut().unwrap();
    finding.insert("severity".into(), json!("CRITICAL"));
    finding.insert("confidence".into(), json!("HIGH"));
    finding.insert("release_impact".into(), json!("BLOCKER"));
    // technical_assessment stays READY with empty sets.
    let violations = vr7_violations(&report);
    assert!(
        violations.iter().any(|v| v.locator == "technical_assessment.result"),
        "CRITICAL+HIGH+BLOCKER recorded READY must raise VR7; got {violations:?}"
    );
}

/// Set comparison is order-insensitive: a recorded set equal up to ordering does
/// NOT raise VR7. Two blocking gates recomputed, recorded in the opposite order.
#[test]
fn set_comparison_is_order_insensitive() {
    let mut report = golden_mut();
    // Two required gates both FAIL -> blocking_gates = {Tests, Build}.
    report["gates"][0]["state"] = json!("FAIL"); // Tests
    report["gates"].as_array_mut().unwrap().push(json!({
        "name": "Build",
        "requirement_level": "required",
        "state": "FAIL",
        "rationale": "broken",
        "support_mappings": [],
        "supporting_findings": []
    }));
    // Record the correct NOT_READY result and blocking_gates in reversed order.
    report["technical_assessment"]["result"] = json!("NOT_READY");
    report["technical_assessment"]["blocking_gates"] = json!(["Build", "Tests"]);

    let violations = vr7_violations(&report);
    assert!(
        !violations
            .iter()
            .any(|v| v.locator == "technical_assessment.blocking_gates"),
        "reordered-but-equal blocking_gates must NOT raise VR7; got {violations:?}"
    );
}
