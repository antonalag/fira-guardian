//! Execution-grounded Tests + Build gate evaluation (vertical slice).
//!
//! Pure CORE tests over `fira_core::gate_eval`: the §6 outcome→epistemic subset,
//! the VR1 sufficiency rule, and the §7 decision table (PASS/FAIL/UNKNOWN) for
//! the two mechanism-backed gates only. Computed gates are also fed through the
//! unchanged verdict engine to confirm integration. No process/FS/network.

use fira_core::execution::{ExecutionResult, VerificationMechanism};
use fira_core::gate_eval::{
    backing_gate, evaluate_execution_backed_gates, ExecutedMechanismResult,
};
use fira_core::model::{
    AlwaysTrue, EpistemicConclusion, ExecutionOutcome, Facet, GateName, GateState, IdRef,
    RequirementLevel, Sufficiency, TechnicalAssessmentResult, VerificationMechanismSource,
};
use fira_core::verdict::{compute_assessment, ExpectationViolations};

fn mech(id: &str) -> VerificationMechanism {
    VerificationMechanism {
        id: IdRef(id.to_string()),
        source: VerificationMechanismSource::Npm,
        source_locator: "package.json".to_string(),
        command: format!("npm run {id}"),
        declared_by_project: AlwaysTrue,
    }
}

fn result(id: &str, outcome: ExecutionOutcome) -> ExecutionResult {
    ExecutionResult {
        command_id: IdRef(id.to_string()),
        command: format!("npm run {id}"),
        outcome,
        classification: None,
        blocking_condition: None,
        captured_output: String::new(),
        exercised_behaviors: Vec::new(),
    }
}

/// The default library-ish profile gate set used by most tests: Tests + Build
/// required, plus one unrelated required gate (Security) and one N/A gate.
fn profile_gates() -> Vec<(GateName, RequirementLevel)> {
    vec![
        (GateName::Build, RequirementLevel::Required),
        (GateName::Tests, RequirementLevel::Required),
        (GateName::Security, RequirementLevel::Required),
        (GateName::Observability, RequirementLevel::NotApplicable),
    ]
}

fn eval(pairs: &[(VerificationMechanism, ExecutionResult)]) -> Vec<fira_core::gate::Gate> {
    let backed: Vec<ExecutedMechanismResult<'_>> = pairs
        .iter()
        .map(|(m, r)| ExecutedMechanismResult {
            mechanism: m,
            result: r,
        })
        .collect();
    evaluate_execution_backed_gates(&profile_gates(), &backed)
}

fn gate(gates: &[fira_core::gate::Gate], name: GateName) -> Option<&fira_core::gate::Gate> {
    gates.iter().find(|g| g.name == name)
}

// ---------------------------------------------------------------------------
// backing_gate association (CORE; §7.1 canonical only)
// ---------------------------------------------------------------------------

#[test]
fn backing_gate_matrix() {
    assert_eq!(backing_gate(&mech("npm-test")), Some(GateName::Tests));
    assert_eq!(backing_gate(&mech("cargo-test")), Some(GateName::Tests));
    assert_eq!(backing_gate(&mech("make-test")), Some(GateName::Tests));
    assert_eq!(backing_gate(&mech("npm-build")), Some(GateName::Build));
    assert_eq!(backing_gate(&mech("cargo-build")), Some(GateName::Build));
    assert_eq!(backing_gate(&mech("make-build")), Some(GateName::Build));
    // Variants / linters / typecheckers / unknown back NOTHING (no PASS inferred).
    assert_eq!(backing_gate(&mech("npm-test:coverage")), None);
    assert_eq!(backing_gate(&mech("npm-lint")), None);
    assert_eq!(backing_gate(&mech("npm-typecheck")), None);
    assert_eq!(backing_gate(&mech("npm-test:watch")), None);
    assert_eq!(backing_gate(&mech("npm-dev")), None);
}

// ---------------------------------------------------------------------------
// PASS / FAIL / UNKNOWN
// ---------------------------------------------------------------------------

/// AC-1: PASSED backing mechanism ⇒ gate PASS with ≥1 SUFFICIENT mapping + an
/// OBSERVED/VERIFIED execution-grounded epistemic state.
#[test]
fn passed_yields_pass_with_sufficient_evidence() {
    let gates = eval(&[
        (
            mech("npm-test"),
            result("npm-test", ExecutionOutcome::Passed),
        ),
        (
            mech("npm-build"),
            result("npm-build", ExecutionOutcome::Passed),
        ),
    ]);
    for name in [GateName::Tests, GateName::Build] {
        let g = gate(&gates, name).unwrap_or_else(|| panic!("missing {name:?}"));
        assert_eq!(g.state, GateState::Pass, "{name:?} should PASS");
        assert!(
            g.support_mappings
                .iter()
                .any(|m| m.sufficiency == Sufficiency::Sufficient),
            "PASS carries ≥1 SUFFICIENT mapping (VR1)"
        );
    }
    // The execution-grounded epistemic state is OBSERVED + VERIFIED (VR11/C6).
    let st =
        fira_core::gate_eval::observed_state_for(&result("npm-test", ExecutionOutcome::Passed))
            .expect("passed ⇒ observed state");
    assert!(st.facets.contains(&Facet::Observed));
    assert_eq!(st.conclusion, EpistemicConclusion::Verified);
}

/// AC-2: FAILED backing mechanism ⇒ gate FAIL (app-level negative), never PASS.
#[test]
fn failed_yields_fail() {
    let gates = eval(&[(
        mech("npm-test"),
        result("npm-test", ExecutionOutcome::Failed),
    )]);
    let g = gate(&gates, GateName::Tests).unwrap();
    assert_eq!(g.state, GateState::Fail);
    assert_ne!(g.state, GateState::Pass);
}

/// AC-3: a gate with no recognized backing mechanism is not produced by the
/// evaluator (the caller leaves it UNKNOWN). Security has no backing mechanism.
#[test]
fn unbacked_gate_not_evaluated() {
    let gates = eval(&[(
        mech("npm-test"),
        result("npm-test", ExecutionOutcome::Passed),
    )]);
    assert!(
        gate(&gates, GateName::Security).is_none(),
        "Security is not backed"
    );
    // Only Tests (backed+passed) is produced here; Build has no backing mechanism
    // in this input, so it is also not produced.
    assert!(
        gate(&gates, GateName::Build).is_none(),
        "Build unbacked here"
    );
    assert!(gate(&gates, GateName::Tests).is_some());
}

/// AC-4: NOT_RUN / TIMEOUT / BLOCKED / ERROR ⇒ UNKNOWN, never PASS, never FAIL.
#[test]
fn non_conclusive_outcomes_yield_unknown_never_pass() {
    for outcome in [
        ExecutionOutcome::NotRun,
        ExecutionOutcome::Timeout,
        ExecutionOutcome::Blocked,
        ExecutionOutcome::Error,
    ] {
        let gates = eval(&[(mech("npm-test"), result("npm-test", outcome))]);
        let g = gate(&gates, GateName::Tests).unwrap();
        assert_eq!(g.state, GateState::Unknown, "{outcome:?} ⇒ UNKNOWN");
        assert_ne!(g.state, GateState::Pass, "{outcome:?} must never PASS");
        assert_ne!(g.state, GateState::Fail, "{outcome:?} is not app-negative");
        // Its mapping is INSUFFICIENT (so a PASS could never rest on it).
        assert!(g
            .support_mappings
            .iter()
            .all(|m| m.sufficiency == Sufficiency::Insufficient));
    }
}

/// AC-5 / §7.2: when both a PASS and a FAIL back the same gate, app-level
/// negative dominates ⇒ FAIL (fail-safe), never PASS.
#[test]
fn mixed_outcomes_fail_dominates() {
    let gates = eval(&[
        (
            mech("npm-test"),
            result("npm-test", ExecutionOutcome::Passed),
        ),
        // (hypothetical second canonical backing with a failure)
        (
            mech("cargo-test"),
            result("cargo-test", ExecutionOutcome::Failed),
        ),
    ]);
    let g = gate(&gates, GateName::Tests).unwrap();
    assert_eq!(
        g.state,
        GateState::Fail,
        "a failing backing mechanism dominates"
    );
}

/// AC-6: only Tests/Build are ever evaluated; a mechanism that is not a
/// recognized test/build backs nothing, so no other gate is produced.
#[test]
fn only_tests_and_build_are_evaluated() {
    let gates = eval(&[
        (
            mech("npm-lint"),
            result("npm-lint", ExecutionOutcome::Passed),
        ),
        (
            mech("npm-typecheck"),
            result("npm-typecheck", ExecutionOutcome::Passed),
        ),
    ]);
    assert!(
        gates.is_empty(),
        "lint/typecheck back no gate in this slice"
    );
}

// ---------------------------------------------------------------------------
// Determinism + verdict integration
// ---------------------------------------------------------------------------

/// AC-10: deterministic — same inputs ⇒ identical gates.
#[test]
fn deterministic() {
    let pairs = [
        (
            mech("npm-test"),
            result("npm-test", ExecutionOutcome::Passed),
        ),
        (
            mech("npm-build"),
            result("npm-build", ExecutionOutcome::Passed),
        ),
    ];
    assert_eq!(eval(&pairs), eval(&pairs));
}

/// AC-9: feeding the evaluated gates through the UNCHANGED verdict engine.
/// Tests+Build PASS but a required Security gate is UNKNOWN ⇒ NOT_READY (B4).
#[test]
fn verdict_integration_required_unknown_blocks() {
    use fira_core::gate::Gate;
    use fira_core::model::GateCause;

    let mut gates = eval(&[
        (
            mech("npm-test"),
            result("npm-test", ExecutionOutcome::Passed),
        ),
        (
            mech("npm-build"),
            result("npm-build", ExecutionOutcome::Passed),
        ),
    ]);
    // The caller leaves the unbacked required Security gate UNKNOWN.
    gates.push(Gate {
        name: GateName::Security,
        requirement_level: RequirementLevel::Required,
        state: GateState::Unknown,
        rationale: "not sufficiently audited".to_string(),
        cause: Some(GateCause::None),
        support_mappings: Vec::new(),
        supporting_findings: Vec::new(),
    });

    let a = compute_assessment(&gates, &[], &ExpectationViolations::none());
    assert_eq!(
        a.result,
        TechnicalAssessmentResult::NotReady,
        "a required UNKNOWN gate blocks (B4) even with Tests/Build PASS"
    );
}

/// Tests+Build PASS and every other required gate PASS ⇒ READY (shows PASS is
/// real, not cosmetic).
#[test]
fn verdict_ready_when_all_required_pass() {
    use fira_core::gate::Gate;
    use fira_core::model::GateCause;

    let mut gates = eval(&[
        (
            mech("npm-test"),
            result("npm-test", ExecutionOutcome::Passed),
        ),
        (
            mech("npm-build"),
            result("npm-build", ExecutionOutcome::Passed),
        ),
    ]);
    gates.push(Gate {
        name: GateName::Security,
        requirement_level: RequirementLevel::Recommended,
        state: GateState::Pass,
        rationale: "r".to_string(),
        cause: Some(GateCause::None),
        support_mappings: vec![],
        supporting_findings: Vec::new(),
    });
    let a = compute_assessment(&gates, &[], &ExpectationViolations::none());
    assert_eq!(a.result, TechnicalAssessmentResult::Ready);
}
