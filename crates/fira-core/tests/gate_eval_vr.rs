//! VR gauntlet for the execution-grounded gate slice: an evaluated PASS gate
//! (built by `gate_eval` from a PASSED mechanism) must satisfy the semantic
//! validators (VR1 sufficiency, VR11 execution-based VERIFIED, VR12 command FK)
//! when spliced into a real report — and a deliberately-broken PASS (no
//! SUFFICIENT mapping) must be rejected by VR1. This proves the slice's evidence
//! is well-formed, not merely asserted.

mod common;

use common::load_json;
use serde_json::{json, Value};

use fira_core::execution::{ExecutionResult, VerificationMechanism};
use fira_core::gate_eval::{evaluate_execution_backed_gates, ExecutedMechanismResult};
use fira_core::model::{
    AlwaysTrue, ExecutionOutcome, GateName, IdRef, RequirementLevel, VerificationMechanismSource,
};
use fira_core::validation::{run_vrs, StaticValidationContext, Vr};

fn mech(id: &str) -> VerificationMechanism {
    VerificationMechanism {
        id: IdRef(id.to_string()),
        source: VerificationMechanismSource::Npm,
        source_locator: "package.json".to_string(),
        command: format!("npm run {id}"),
        declared_by_project: AlwaysTrue,
    }
}

fn passed(id: &str) -> ExecutionResult {
    ExecutionResult {
        command_id: IdRef(id.to_string()),
        command: format!("npm run {id}"),
        outcome: ExecutionOutcome::Passed,
        classification: None,
        blocking_condition: None,
        captured_output: String::new(),
        exercised_behaviors: Vec::new(),
    }
}

/// Build the evaluated Tests gate (PASS) for an `npm-test` PASSED mechanism, and
/// splice it + its backing mechanism into the golden report. Returns the mutated
/// report `Value` so VRs can run over it.
fn report_with_evaluated_tests_gate() -> Value {
    let m = mech("npm-test");
    let r = passed("npm-test");
    let backed = [ExecutedMechanismResult {
        mechanism: &m,
        result: &r,
    }];
    let gates =
        evaluate_execution_backed_gates(&[(GateName::Tests, RequirementLevel::Required)], &backed);
    assert_eq!(gates.len(), 1, "exactly the Tests gate is evaluated");

    let gate_json = serde_json::to_value(&gates[0]).expect("gate serializes");

    let mut report = load_json("schemas/examples/valid/s11-audit-report.json");
    // The gate must map to ≥1 coverage entry (VR8): the golden report's single
    // gate is Tests, which we replace, so its existing coverage entry still
    // applies by name.
    report["gates"] = Value::Array(vec![gate_json]);
    // VR12: the gate's evidence references `npm-test`, so that mechanism must be
    // in the registry as declared_by_project. APPEND it to the golden report's
    // existing mechanisms (do not replace — the golden coverage's
    // executed_mechanisms still FK to the original `cmd-tests`, VR12).
    if let Some(mechs) = report["verification_mechanisms"].as_array_mut() {
        mechs.push(json!({
            "id": "npm-test",
            "source": "NPM",
            "source_locator": "package.json",
            "command": "npm run npm-test",
            "declared_by_project": true
        }));
    }
    report
}

/// The evaluated PASS gate passes VR1/VR11/VR12 (and all run_vrs checks) when
/// spliced into a real report with its backing mechanism registered.
#[test]
fn evaluated_pass_gate_satisfies_vrs() {
    let report = report_with_evaluated_tests_gate();
    let ctx = StaticValidationContext::from_report(&report);
    let violations = run_vrs(&report, &ctx);
    // No VR1 (sufficiency), VR11 (verified facet), or VR12 (command FK) violation.
    assert!(
        !violations.iter().any(|v| v.vr == Vr::Vr1),
        "evaluated PASS gate must have a SUFFICIENT mapping (VR1); got {violations:?}"
    );
    assert!(
        !violations.iter().any(|v| v.vr == Vr::Vr12),
        "the gate's command evidence must FK to a declared mechanism (VR12); got {violations:?}"
    );
    assert!(
        !violations.iter().any(|v| v.vr == Vr::Vr11),
        "no static-only VERIFIED (VR11); got {violations:?}"
    );
}

/// A deliberately-broken PASS gate — its support mapping downgraded to
/// INSUFFICIENT — must be rejected by VR1 (a PASS can never rest on insufficient
/// evidence). This confirms the validator is actually guarding the slice.
#[test]
fn broken_pass_gate_without_sufficient_is_rejected_by_vr1() {
    let mut report = report_with_evaluated_tests_gate();
    // Downgrade every support mapping on the gate to INSUFFICIENT.
    if let Some(mappings) = report["gates"][0]["support_mappings"].as_array_mut() {
        for m in mappings.iter_mut() {
            m["sufficiency"] = json!("INSUFFICIENT");
            m["unverified_remainder"] = json!("forced insufficient for the test");
        }
    }
    let ctx = StaticValidationContext::from_report(&report);
    let violations = run_vrs(&report, &ctx);
    assert!(
        violations.iter().any(|v| v.vr == Vr::Vr1),
        "a PASS gate with no SUFFICIENT mapping must be rejected by VR1; got {violations:?}"
    );
}
