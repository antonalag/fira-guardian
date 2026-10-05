//! The pure, deterministic confidence rubric (frozen contract §8; CONF-1/
//! CONF-2; E1/E5).
//!
//! `recompute_confidence_outcome` is a pure function of a [`Finding`]'s
//! structured evidence model. It never reads the finding's recorded
//! `confidence` (CONF-1: confidence is derived, not model-overridable). It
//! implements only the **deterministic** portion of §8 that the structured model
//! can decide; the qualitative MEDIUM vs LOW distinction is intentionally **not**
//! mechanized (the model carries no "indirect"/"adjacent"/"ambiguity" signal —
//! approved Option A, Task 6 §9.2), so publishable-but-not-HIGH is a single
//! bucket here.

use crate::finding::Finding;
use crate::model::{Confidence, Facet, GateName, Sufficiency};

/// The deterministically-decidable confidence outcome for a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceOutcome {
    /// No SUFFICIENT support mapping: the evidence is insufficient to publish as
    /// a FINDING (CONF-2). The finding path is exited (HYPOTHESIS/UNKNOWN); this
    /// is not a confidence level.
    NotPublishable,
    /// Meets the §8 HIGH bar: publishable, with a direct execution-based facet
    /// and — for failure-class properties — the failure case exercised (E5).
    High,
    /// Publishable (≥1 SUFFICIENT) but does not meet the HIGH bar. Per Option A
    /// (§9.2) the MEDIUM/LOW split is not mechanized; a recorded MEDIUM or LOW is
    /// consistent with this outcome ("ambiguity resolves downward; never below
    /// LOW into publishable").
    PublishableNonHigh,
}

/// Gate names (frozen contract §20) denoting the failure classes (same set VR2
/// uses). Kept identical to `validation::rules::vr02_failure_anchor` so the
/// failure-class notion is one consistent, contract-fixed classification.
const FAILURE_CLASS_GATES: [GateName; 3] = [
    GateName::Concurrency,
    GateName::PersistenceDurability,
    GateName::FailureRecovery,
];

/// Lowercase category tokens denoting the failure classes (same set VR2 uses).
const FAILURE_CLASS_TOKENS: [&str; 3] = ["concurrency", "durability", "recovery"];

/// True if the finding is failure-class (concurrency/durability/recovery),
/// decided exactly as VR2 decides it: a category token match OR a
/// failure-class gate in `gates_affected`.
fn is_failure_class(finding: &Finding) -> bool {
    let category_hit = {
        let lower = finding.category.to_ascii_lowercase();
        FAILURE_CLASS_TOKENS.iter().any(|t| lower.contains(t))
    };
    let gate_hit = finding
        .gates_affected
        .iter()
        .any(|g| FAILURE_CLASS_GATES.contains(g));
    category_hit || gate_hit
}

/// True if at least one support mapping is SUFFICIENT (CONF-2 publish gate).
fn has_sufficient_mapping(finding: &Finding) -> bool {
    finding
        .support_mappings
        .iter()
        .any(|m| m.sufficiency == Sufficiency::Sufficient)
}

/// True if the finding's epistemic state carries an execution-based facet.
///
/// OBSERVED is the unambiguously execution-based facet from the facet set alone
/// (consistent with the mechanical VR11 reading); "executed-PASSED TESTED" needs
/// an ExecutionResult link that is not mechanized here (same deferral as VR11).
fn has_execution_based_facet(finding: &Finding) -> bool {
    finding.epistemic_state.facets.contains(&Facet::Observed)
}

/// True if a failure scenario with a non-empty anchor is present (E5: the
/// failure case must be exercised/anchored for failure-class HIGH).
fn has_failure_case(finding: &Finding) -> bool {
    finding
        .failure_scenario
        .as_ref()
        .map(|fs| !fs.anchor.is_empty())
        .unwrap_or(false)
}

/// Recompute a finding's confidence outcome from its evidence model. Pure and
/// deterministic (CONF-1): same `finding` ⇒ same result; no clock, randomness,
/// filesystem, process, or network.
pub fn recompute_confidence_outcome(finding: &Finding) -> ConfidenceOutcome {
    // CONF-2 publish gate: without a SUFFICIENT mapping, not a published finding.
    if !has_sufficient_mapping(finding) {
        return ConfidenceOutcome::NotPublishable;
    }

    // §8 HIGH bar (E1/E5): direct execution-based evidence, and for failure-class
    // properties the failure case exercised.
    let meets_high = has_execution_based_facet(finding)
        && (!is_failure_class(finding) || has_failure_case(finding));
    if meets_high {
        return ConfidenceOutcome::High;
    }

    // Publishable but not HIGH. The MEDIUM/LOW split is not mechanized (Option A).
    ConfidenceOutcome::PublishableNonHigh
}

/// Whether a recorded [`Confidence`] contradicts the recomputed
/// [`ConfidenceOutcome`] in a way the frozen contract makes **determinable**.
///
/// This is the VR5 compare step. It validates every determinable aspect — it
/// does **not** collapse into "HIGH or not HIGH":
///
/// - `NotPublishable` recorded as **any** confidence is a contradiction (a
///   published finding with no SUFFICIENT mapping — CONF-2/VR1 overlap).
/// - A recorded **HIGH** that does not meet the HIGH bar is a contradiction
///   (overclaimed HIGH; includes the failure-class failure-case requirement).
/// - A recorded **MEDIUM/LOW** against a recomputed `High` or `PublishableNonHigh`
///   is **not** flagged: the MEDIUM/LOW distinction is not deterministically
///   decidable (Option A §9.2/§9.3), and downward resolution (§8/CONF-1) keeps a
///   lower recorded value defensible.
pub fn recorded_contradicts(recorded: Confidence, outcome: ConfidenceOutcome) -> bool {
    match outcome {
        // Not publishable: any recorded confidence level is a contradiction.
        ConfidenceOutcome::NotPublishable => true,
        // Evidence does not meet HIGH: a recorded HIGH is overclaimed.
        ConfidenceOutcome::PublishableNonHigh => recorded == Confidence::High,
        // Evidence meets HIGH: HIGH/MEDIUM/LOW are all consistent (downward ok).
        ConfidenceOutcome::High => false,
    }
}
