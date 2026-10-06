//! The pure, deterministic verdict engine (frozen contract §11; C10).
//!
//! `compute_assessment` implements §11 Steps 1–4 as a pure function of the
//! structured gates + findings. It never reads the report's recorded
//! `technical_assessment` (C10: the verdict is recomputed, not trusted). Gate
//! states are consumed as recorded (`Gate.state`); §7 gate-state derivation
//! (Step0) is a separate concern and is not re-derived here (Task 7 §9.2(A)).
//! `minimum_evidence_expectations` are POLICY (Task 8); until they exist, the
//! caller supplies [`ExpectationViolations`] (default empty), so required
//! PARTIAL resolves to RISK — exactly §11's `else` branch (§9.1(A)).

use crate::finding::Finding;
use crate::gate::Gate;
use crate::model::{
    Confidence, FindingId, GateName, GateState, HumanDecisionFiraState, LifecycleStatus,
    ReleaseImpact, RequirementLevel, Severity, TechnicalAssessmentResult,
};

/// Required gates known to violate a `minimum_evidence_expectation`. Supplied by
/// the caller; **not** POLICY data authored here. Default empty ⇒ no required
/// PARTIAL is forced to block (it becomes RISK, §11 `else`). When Task 8 POLICY
/// exists, a caller may populate this; Task 7 ships no expectations.
#[derive(Debug, Clone, Default)]
pub struct ExpectationViolations {
    gates: Vec<GateName>,
}

impl ExpectationViolations {
    /// An empty set (no known expectation violations).
    pub fn none() -> Self {
        ExpectationViolations { gates: Vec::new() }
    }

    /// Build from a list of gates known to violate an expectation.
    pub fn from_gates(gates: Vec<GateName>) -> Self {
        ExpectationViolations { gates }
    }

    fn contains(&self, gate: &GateName) -> bool {
        self.gates.contains(gate)
    }
}

/// The recomputed assessment: the §11 result and the deterministic blocking/risk
/// sets, plus the FIRA-set human decision for the branches §11 fixes.
///
/// `human_decision` is `Some(..)` only where §11 Step3 fixes it: READY ⇒
/// `NOT_APPLICABLE`, READY_WITH_RISKS ⇒ `PENDING`. For `NOT_READY` the frozen
/// contract does not fix a FIRA HumanDecision (Step3 specifies it only for the
/// READY / READY_WITH_RISKS branches; §10 composes `NOT_READY + any → NOT_READY`),
/// so it is left `None` rather than inventing a value. FIRA only ever produces
/// `NOT_APPLICABLE | PENDING` (VR6) — never ACCEPTED/REJECTED.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecomputedAssessment {
    pub result: TechnicalAssessmentResult,
    pub blocking_findings: Vec<FindingId>,
    pub blocking_gates: Vec<GateName>,
    pub risk_findings: Vec<FindingId>,
    pub risk_gates: Vec<GateName>,
    pub human_decision: Option<HumanDecisionFiraState>,
}

/// Whether a finding participates in the verdict given its lifecycle (§11 Step4,
/// §14): `VERIFIED_FIXED` is excluded; `REOPENED` is evaluated as `OPEN`;
/// `WONT_FIX` is retained (keeps its severity, still enters blocking/risks);
/// `OPEN` and `FIX_CLAIMED` participate (FIX_CLAIMED is verdict-equivalent to
/// OPEN until independently verified as fixed — §14).
fn finding_participates(finding: &Finding) -> bool {
    !matches!(finding.lifecycle_status, LifecycleStatus::VerifiedFixed)
}

/// The requirement level of a gate by name, if present in the report's gates.
fn requirement_level_of(gates: &[Gate], name: &GateName) -> Option<RequirementLevel> {
    gates
        .iter()
        .find(|g| g.name == *name)
        .map(|g| g.requirement_level)
}

/// True if the finding affects at least one **required** gate (used by B2).
fn affects_required_gate(finding: &Finding, gates: &[Gate]) -> bool {
    finding.gates_affected.iter().any(|gn| {
        requirement_level_of(gates, gn) == Some(RequirementLevel::Required)
    })
}

/// Push `id` into `set` if not already present (order-preserving dedupe).
fn push_unique<T: PartialEq + Clone>(set: &mut Vec<T>, id: &T) {
    if !set.contains(id) {
        set.push(id.clone());
    }
}

/// Compute the deterministic verdict (§11 Steps 1–4) from the report's gates and
/// findings. Pure and deterministic (C10): same inputs ⇒ same output; no clock,
/// randomness, filesystem, process, or network. Does not read any recorded
/// assessment.
pub fn compute_assessment(
    gates: &[Gate],
    findings: &[Finding],
    expectation_violations: &ExpectationViolations,
) -> RecomputedAssessment {
    let mut blocking_findings: Vec<FindingId> = Vec::new();
    let mut blocking_gates: Vec<GateName> = Vec::new();
    let mut risk_findings: Vec<FindingId> = Vec::new();
    let mut risk_gates: Vec<GateName> = Vec::new();

    // --- Findings (Step1 B1/B2, Step2 release_impact=RISK), after Step4 filter.
    // Step4: multiple MEDIUMs never aggregate by count — each finding is judged
    // on its own, so there is no counting anywhere below.
    for finding in findings.iter().filter(|f| finding_participates(f)) {
        let is_blocker = finding.release_impact == ReleaseImpact::Blocker;

        // B1: CRITICAL + {HIGH,MEDIUM} + BLOCKER ⇒ block. CRITICAL+LOW does not
        // auto-block (its "forbids PASS on its gates" clause is a §7 gate-state
        // concern, out of scope under §9.2(A)).
        let b1 = finding.severity == Severity::Critical
            && matches!(finding.confidence, Confidence::High | Confidence::Medium)
            && is_blocker;

        // B2: HIGH + BLOCKER on a required gate ⇒ block.
        let b2 = finding.severity == Severity::High
            && is_blocker
            && affects_required_gate(finding, gates);

        if b1 || b2 {
            push_unique(&mut blocking_findings, &finding.id);
        } else if finding.release_impact == ReleaseImpact::Risk {
            // Step2: findings with release_impact=RISK are risks.
            push_unique(&mut risk_findings, &finding.id);
        }
    }

    // --- Gates (Step1 B3/B4/B5, Step2 recommended-gate risks), Step4 N/A guard.
    for gate in gates {
        // Step4: N/A gates never enter blocking/risks.
        if gate.state == GateState::NotApplicable {
            continue;
        }

        match gate.requirement_level {
            RequirementLevel::Required => match gate.state {
                // B3 required FAIL ⇒ block; B4 required UNKNOWN ⇒ block (incl.
                // infra-caused UNKNOWN, which still blocks on a required gate).
                GateState::Fail | GateState::Unknown => {
                    push_unique(&mut blocking_gates, &gate.name);
                }
                // B5 required PARTIAL ⇒ block iff it violates a
                // minimum_evidence_expectation (caller-supplied); else RISK.
                GateState::Partial => {
                    if expectation_violations.contains(&gate.name) {
                        push_unique(&mut blocking_gates, &gate.name);
                    } else {
                        push_unique(&mut risk_gates, &gate.name);
                    }
                }
                GateState::Pass | GateState::NotApplicable => {}
            },
            RequirementLevel::Recommended => {
                // Step2: recommended gates in {UNKNOWN, FAIL, PARTIAL} ⇒ risk
                // (includes infra-caused UNKNOWN on a recommended gate ⇒ risk).
                if matches!(
                    gate.state,
                    GateState::Unknown | GateState::Fail | GateState::Partial
                ) {
                    push_unique(&mut risk_gates, &gate.name);
                }
            }
            // not_applicable requirement level: the gate's state is N/A (VR4),
            // already skipped above; nothing to do.
            RequirementLevel::NotApplicable => {}
        }
    }

    // --- Step3 first-match result + FIRA HumanDecision (VR6).
    let blocking_nonempty = !blocking_findings.is_empty() || !blocking_gates.is_empty();
    let risks_nonempty = !risk_findings.is_empty() || !risk_gates.is_empty();

    let (result, human_decision) = if blocking_nonempty {
        // NOT_READY. §11 Step3 does not fix a FIRA HumanDecision here, and §10
        // composes NOT_READY + any → NOT_READY, so none is asserted. Human
        // acceptance (external) never turns NOT_READY into a release-ready result.
        (TechnicalAssessmentResult::NotReady, None)
    } else if risks_nonempty {
        (
            TechnicalAssessmentResult::ReadyWithRisks,
            Some(HumanDecisionFiraState::Pending),
        )
    } else {
        (
            TechnicalAssessmentResult::Ready,
            Some(HumanDecisionFiraState::NotApplicable),
        )
    };

    RecomputedAssessment {
        result,
        blocking_findings,
        blocking_gates,
        risk_findings,
        risk_gates,
        human_decision,
    }
}
