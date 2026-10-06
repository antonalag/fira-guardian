//! AC-1, AC-2, AC-5, AC-6: unit tests for the pure deterministic verdict engine.
//!
//! Each §11 rule is exercised positively and negatively over the typed model,
//! without JSON. Findings/gates are built from JSON for brevity (also exercising
//! real deserialization) and fed to `compute_assessment`.

use serde_json::{json, Value};

use fira_core::finding::Finding;
use fira_core::gate::Gate;
use fira_core::model::{GateName, HumanDecisionFiraState, TechnicalAssessmentResult};
use fira_core::verdict::{compute_assessment, ExpectationViolations};

/// A publishable base finding: LOW severity, HIGH confidence, NON_BLOCKING, OPEN,
/// affecting the Tests gate. Tests override only what they need.
fn finding(overrides: Value) -> Finding {
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
        "epistemic_state": { "facets": ["OBSERVED"], "conclusion": "VERIFIED" },
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
    merge(&mut base, overrides);
    serde_json::from_value(base).expect("finding deserializes")
}

/// A gate with the given name/level/state. Tests override as needed.
fn gate(name: &str, level: &str, state: &str) -> Gate {
    serde_json::from_value(json!({
        "name": name,
        "requirement_level": level,
        "state": state,
        "rationale": "r",
        "support_mappings": [],
        "supporting_findings": []
    }))
    .expect("gate deserializes")
}

fn gate_with(value: Value) -> Gate {
    serde_json::from_value(value).expect("gate deserializes")
}

fn merge(base: &mut Value, overrides: Value) {
    let obj = base.as_object_mut().unwrap();
    for (k, v) in overrides.as_object().unwrap() {
        obj.insert(k.clone(), v.clone());
    }
}

fn none() -> ExpectationViolations {
    ExpectationViolations::none()
}

// --- Step3 baseline ---------------------------------------------------------

#[test]
fn clean_report_is_ready_not_applicable() {
    let gates = vec![gate("Tests", "required", "PASS")];
    let findings = vec![finding(json!({}))];
    let a = compute_assessment(&gates, &findings, &none());
    assert_eq!(a.result, TechnicalAssessmentResult::Ready);
    assert_eq!(a.human_decision, Some(HumanDecisionFiraState::NotApplicable));
    assert!(a.blocking_findings.is_empty() && a.blocking_gates.is_empty());
    assert!(a.risk_findings.is_empty() && a.risk_gates.is_empty());
}

// --- B1: CRITICAL + {HIGH,MEDIUM} + BLOCKER ---------------------------------

#[test]
fn b1_critical_high_blocker_blocks() {
    let gates = vec![gate("Tests", "required", "PASS")];
    let findings = vec![finding(json!({
        "severity": "CRITICAL", "confidence": "HIGH", "release_impact": "BLOCKER"
    }))];
    let a = compute_assessment(&gates, &findings, &none());
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady);
    assert_eq!(a.blocking_findings, vec![fira_core::model::FindingId("RR-001".into())]);
}

#[test]
fn b1_critical_medium_blocker_blocks() {
    let gates = vec![gate("Tests", "required", "PASS")];
    let findings = vec![finding(json!({
        "severity": "CRITICAL", "confidence": "MEDIUM", "release_impact": "BLOCKER"
    }))];
    let a = compute_assessment(&gates, &findings, &none());
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady);
}

#[test]
fn b1_critical_low_blocker_does_not_auto_block() {
    // CRITICAL+LOW+BLOCKER does not auto-block (§11 B1). With no other blocking
    // input and release_impact=BLOCKER (not RISK), it is neither blocking nor a
    // risk here, so the result is READY.
    let gates = vec![gate("Tests", "required", "PASS")];
    let findings = vec![finding(json!({
        "severity": "CRITICAL", "confidence": "LOW", "release_impact": "BLOCKER"
    }))];
    let a = compute_assessment(&gates, &findings, &none());
    assert!(a.blocking_findings.is_empty(), "CRITICAL+LOW must not auto-block");
    assert_eq!(a.result, TechnicalAssessmentResult::Ready);
}

// --- B2: HIGH + BLOCKER on a required gate ----------------------------------

#[test]
fn b2_high_blocker_on_required_gate_blocks() {
    let gates = vec![gate("Tests", "required", "PASS")];
    let findings = vec![finding(json!({
        "severity": "HIGH", "release_impact": "BLOCKER", "gates_affected": ["Tests"]
    }))];
    let a = compute_assessment(&gates, &findings, &none());
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady);
    assert!(a.blocking_findings.contains(&fira_core::model::FindingId("RR-001".into())));
}

#[test]
fn b2_high_blocker_on_recommended_gate_does_not_block_via_b2() {
    // The affected gate is recommended, so B2 does not fire. release_impact is
    // BLOCKER (not RISK), so it is not a finding-risk either; the recommended
    // gate is PASS so no gate-risk. Result READY.
    let gates = vec![gate("Documentation", "recommended", "PASS")];
    let findings = vec![finding(json!({
        "severity": "HIGH", "release_impact": "BLOCKER", "gates_affected": ["Documentation"]
    }))];
    let a = compute_assessment(&gates, &findings, &none());
    assert!(a.blocking_findings.is_empty(), "B2 requires a required gate");
    assert_eq!(a.result, TechnicalAssessmentResult::Ready);
}

// --- B3 / B4: required FAIL / UNKNOWN block ---------------------------------

#[test]
fn b3_required_fail_blocks() {
    let gates = vec![gate("Tests", "required", "FAIL")];
    let a = compute_assessment(&gates, &[], &none());
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady);
    assert!(a.blocking_gates.contains(&GateName::Tests));
}

#[test]
fn b4_required_unknown_blocks() {
    let gates = vec![gate("Tests", "required", "UNKNOWN")];
    let a = compute_assessment(&gates, &[], &none());
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady);
    assert!(a.blocking_gates.contains(&GateName::Tests));
}

#[test]
fn b4_required_unknown_infra_still_blocks() {
    // Infra-caused UNKNOWN on a required gate still blocks (§11 Step2).
    let gates = vec![gate_with(json!({
        "name": "Tests", "requirement_level": "required", "state": "UNKNOWN",
        "rationale": "env missing", "cause": "INFRA",
        "support_mappings": [], "supporting_findings": []
    }))];
    let a = compute_assessment(&gates, &[], &none());
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady);
    assert!(a.blocking_gates.contains(&GateName::Tests));
}

// --- B5: required PARTIAL — RISK by default, blocks on expectation violation --

#[test]
fn b5_required_partial_is_risk_by_default() {
    let gates = vec![gate("Tests", "required", "PARTIAL")];
    let a = compute_assessment(&gates, &[], &none());
    assert_eq!(a.result, TechnicalAssessmentResult::ReadyWithRisks);
    assert!(a.risk_gates.contains(&GateName::Tests));
    assert!(a.blocking_gates.is_empty());
    assert_eq!(a.human_decision, Some(HumanDecisionFiraState::Pending));
}

#[test]
fn b5_required_partial_blocks_when_expectation_violated() {
    let gates = vec![gate("Tests", "required", "PARTIAL")];
    let ev = ExpectationViolations::from_gates(vec![GateName::Tests]);
    let a = compute_assessment(&gates, &[], &ev);
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady);
    assert!(a.blocking_gates.contains(&GateName::Tests));
    assert!(a.risk_gates.is_empty());
}

// --- Step2: recommended gates & finding RISK --------------------------------

#[test]
fn recommended_unknown_is_risk() {
    let gates = vec![gate("Documentation", "recommended", "UNKNOWN")];
    let a = compute_assessment(&gates, &[], &none());
    assert_eq!(a.result, TechnicalAssessmentResult::ReadyWithRisks);
    assert!(a.risk_gates.contains(&GateName::Documentation));
    assert!(a.blocking_gates.is_empty());
}

#[test]
fn finding_release_impact_risk_is_risk() {
    let gates = vec![gate("Tests", "required", "PASS")];
    let findings = vec![finding(json!({ "release_impact": "RISK" }))];
    let a = compute_assessment(&gates, &findings, &none());
    assert_eq!(a.result, TechnicalAssessmentResult::ReadyWithRisks);
    assert!(a.risk_findings.contains(&fira_core::model::FindingId("RR-001".into())));
}

// --- Step4: lifecycle & N/A & no aggregation --------------------------------

#[test]
fn verified_fixed_finding_excluded() {
    let gates = vec![gate("Tests", "required", "PASS")];
    let findings = vec![finding(json!({
        "severity": "CRITICAL", "confidence": "HIGH", "release_impact": "BLOCKER",
        "lifecycle_status": "VERIFIED_FIXED"
    }))];
    let a = compute_assessment(&gates, &findings, &none());
    assert!(a.blocking_findings.is_empty(), "VERIFIED_FIXED must be excluded");
    assert_eq!(a.result, TechnicalAssessmentResult::Ready);
}

#[test]
fn reopened_evaluated_as_open() {
    let gates = vec![gate("Tests", "required", "PASS")];
    let findings = vec![finding(json!({
        "severity": "CRITICAL", "confidence": "HIGH", "release_impact": "BLOCKER",
        "lifecycle_status": "REOPENED"
    }))];
    let a = compute_assessment(&gates, &findings, &none());
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady, "REOPENED evaluated as OPEN");
}

#[test]
fn wont_fix_retains_blocking_weight() {
    let gates = vec![gate("Tests", "required", "PASS")];
    let findings = vec![finding(json!({
        "severity": "CRITICAL", "confidence": "HIGH", "release_impact": "BLOCKER",
        "lifecycle_status": "WONT_FIX"
    }))];
    let a = compute_assessment(&gates, &findings, &none());
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady, "WONT_FIX still blocks");
}

#[test]
fn fix_claimed_has_blocking_weight_like_open() {
    // FIX_CLAIMED is verdict-equivalent to OPEN until independently verified.
    let gates = vec![gate("Tests", "required", "PASS")];
    let findings = vec![finding(json!({
        "severity": "CRITICAL", "confidence": "HIGH", "release_impact": "BLOCKER",
        "lifecycle_status": "FIX_CLAIMED"
    }))];
    let a = compute_assessment(&gates, &findings, &none());
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady, "FIX_CLAIMED blocks like OPEN");
}

#[test]
fn na_gate_never_enters_sets() {
    let gates = vec![
        gate("Tests", "required", "PASS"),
        gate("Observability", "not_applicable", "N/A"),
    ];
    let a = compute_assessment(&gates, &[], &none());
    assert!(a.blocking_gates.is_empty() && a.risk_gates.is_empty());
    assert_eq!(a.result, TechnicalAssessmentResult::Ready);
}

#[test]
fn multiple_mediums_do_not_aggregate() {
    // Two CRITICAL+MEDIUM+NON_BLOCKING findings: individually none blocks (not
    // BLOCKER), and count must never escalate. Result stays READY.
    let gates = vec![gate("Tests", "required", "PASS")];
    let findings = vec![
        finding(json!({ "id": "RR-001", "severity": "CRITICAL", "confidence": "MEDIUM" })),
        finding(json!({ "id": "RR-002", "severity": "CRITICAL", "confidence": "MEDIUM" })),
    ];
    let a = compute_assessment(&gates, &findings, &none());
    assert_eq!(a.result, TechnicalAssessmentResult::Ready, "count must not escalate");
}

#[test]
fn engine_is_pure_and_deterministic() {
    let gates = vec![gate("Tests", "required", "PARTIAL")];
    let findings = vec![finding(json!({ "release_impact": "RISK" }))];
    let a = compute_assessment(&gates, &findings, &none());
    let b = compute_assessment(&gates, &findings, &none());
    assert_eq!(a, b);
}
