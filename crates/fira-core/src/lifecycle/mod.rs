//! Re-audit lifecycle reconciliation (frozen contract §14).
//!
//! A **pure, deterministic** CORE step that assigns each current-round finding
//! its [`LifecycleStatus`] and `regression` flag by reconciling the current
//! findings against an optional prior [`AuditReport`]. It realizes exactly the
//! §14 state machine and nothing else:
//!
//! ```text
//! OPEN → FIX_CLAIMED (claim, verdict-equivalent to OPEN)
//!              → VERIFIED_FIXED (new valid+SUFFICIENT evidence meeting
//!                                verification_criteria, execution-based where the
//!                                original property required execution)
//!              → FIX_CLAIMED    (claim stands while the bar is unmet; MVP
//!                                presence predicate is stable-id only, §9.2)
//! any → REOPENED on regression  (a previously VERIFIED_FIXED issue recurs)
//! OPEN/REOPENED → WONT_FIX      (recorded; retains severity; still blocks)
//! ```
//!
//! Design rulings realized here (spec §9.1–§9.6):
//! - **§9.1 explicit signal only, from two distinct sources.**
//!   `FIX_CLAIMED`/`WONT_FIX` are honored only from an **explicit recorded**
//!   `lifecycle_status`, read from either (1) the current finding
//!   (`current.lifecycle_status` — the current-round decision) or (2) the matched
//!   prior finding (`prior.lifecycle_status` — the carried-over state). Neither
//!   is inferred from absence, text, location, or severity. No new field.
//! - **§9.2 stable id.** [`FindingId`] is the sole identity/matching key; no
//!   heuristic matching on text/location/severity.
//! - **§9.3 reuse VR11.** Whether the original property required execution is
//!   read via the *same* execution-based-facet predicate [`crate::validation`]
//!   VR11 encodes (OBSERVED) — from the matched prior finding's epistemic state
//!   when a prior exists, otherwise from the current finding's own epistemic
//!   state (a first-time claim describes the same property). There is no second
//!   definition of "requires execution".
//! - **§9.4 absence ≠ fixed.** Absence of a prior id from the current round is
//!   never sufficient for `VERIFIED_FIXED` (lack of evidence is not PASS).
//! - **§9.5 no schema change.** Expressed with the existing
//!   `lifecycle_status`/`regression`/`FindingId` and the existing evidence model.
//! - **§9.6 deterministic.** Pure function of its two arguments; no clock,
//!   randomness, filesystem, process, network, or iteration-order dependence.
//!   The result is keyed and ordered by [`FindingId`].
//!
//! **Precedence (spec design §2.1, first match wins):**
//! 1. **regression** — a matched prior `VERIFIED_FIXED` that recurs ⇒ `REOPENED`
//!    + `regression` (a fact no current claim can mask);
//! 2. **current `WONT_FIX`** — the explicit current-round decision;
//! 3. **fix claim** — current `FIX_CLAIMED`, current `VERIFIED_FIXED`
//!    (claim-at-most, never self-accepted), or carried `FIX_CLAIMED` ⇒
//!    `VERIFIED_FIXED` iff the §14/VR11 evidence bar is independently met, else
//!    `FIX_CLAIMED`;
//! 4. **carried `WONT_FIX`**;
//! 5. **carry-over / default** — carried `OPEN`/`REOPENED`, else `OPEN` (new).
//!
//! A prior report is **historical evidence, never current truth** (§15, C12):
//! its findings are read only to look up a matched finding's prior status and to
//! derive "requires execution"; a prior conclusion never becomes current
//! evidence and never satisfies the
//! [`VERIFIED_FIXED`](LifecycleStatus::VerifiedFixed) bar. Likewise a current
//! `FIX_CLAIMED`/`VERIFIED_FIXED` is a *claim*, never evidence, and a current
//! `WONT_FIX` is an explicit lifecycle decision, never risk acceptance.
//!
//! This module performs **no** filesystem or process I/O (ADR-002): the prior
//! report is handed in as already-loaded data, and the result is returned for
//! the audit flow to apply to the current findings. It does **not** change how
//! the verdict engine weighs these statuses — that logic
//! ([`crate::verdict`]) is unchanged.

use std::collections::BTreeMap;

use crate::evidence::SupportMapping;
use crate::finding::Finding;
use crate::model::{EpistemicConclusion, Facet, FindingId, LifecycleStatus, Sufficiency};
use crate::report::AuditReport;

/// The lifecycle outcome for one current-round finding: the computed status and
/// regression flag the audit flow writes onto the [`Finding`] before the verdict
/// step reads them. Both fields already exist on [`Finding`]; this type carries
/// *only* the lifecycle result, never a new model concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleOutcome {
    /// The stable id of the current-round finding this outcome applies to.
    pub id: FindingId,
    /// The computed §14 lifecycle status.
    pub lifecycle_status: LifecycleStatus,
    /// `true` only when a previously `VERIFIED_FIXED` issue recurred (§14
    /// regression). Never set for any other transition.
    pub regression: bool,
}

/// Reconcile the current round against the prior report (if any) per §14.
///
/// Pure and deterministic (§9.6): the same `current` findings and the same
/// `prior` report yield the same output, independent of the input iteration
/// order (the result is ordered by [`FindingId`]). Does not read the
/// filesystem/process/clock/network, does not mutate `prior`, and never treats a
/// prior conclusion as current evidence (§15, C12).
///
/// `current` carries the post-blind findings and their evidence (§12 P2–P8
/// output); each finding's `lifecycle_status` is read as the explicit
/// current-round signal (§9.1 source 2). `prior` is the already-loaded prior
/// report (Task 11), or `None` for a first audit; a matched prior finding's
/// `lifecycle_status` is the carried-over state (§9.1 source 1). FIX_CLAIMED/
/// WONT_FIX come only from one of those two explicit values; never inferred.
///
/// The returned [`LifecycleOutcome`]s are the lifecycle assignment the caller
/// applies to the current findings; this function neither mutates the inputs nor
/// performs I/O.
pub fn reconcile(current: &[Finding], prior: Option<&AuditReport>) -> Vec<LifecycleOutcome> {
    // Index the prior findings by stable id (§9.2). A BTreeMap keeps the lookup
    // and any iteration deterministic; we only look up by id here.
    let prior_by_id: BTreeMap<&str, &Finding> = match prior {
        Some(report) => report
            .findings
            .iter()
            .map(|f| (f.id.0.as_str(), f))
            .collect(),
        None => BTreeMap::new(),
    };

    // Compute an outcome per current finding, keyed by id so the output order is
    // a deterministic function of the id set alone (§9.6), not of input order.
    let mut by_id: BTreeMap<&str, LifecycleOutcome> = BTreeMap::new();
    for c in current {
        let prior_match = prior_by_id.get(c.id.0.as_str()).copied();
        by_id.insert(c.id.0.as_str(), decide(c, prior_match));
    }

    by_id.into_values().collect()
}

/// Compute the §14 status for one current finding `c`, given its matched prior
/// finding `prior` (if any), by the fixed first-match precedence (design §2.1).
/// `cur` is the explicit current-round signal `c.lifecycle_status`; `prior`
/// contributes the carried-over state and the "requires execution" question.
fn decide(c: &Finding, prior: Option<&Finding>) -> LifecycleOutcome {
    let cur = c.lifecycle_status;
    let prior_status = prior.map(|p| p.lifecycle_status);

    // Rule 1 — regression (highest precedence). A matched prior VERIFIED_FIXED
    // whose id recurs this round is a regression (§14 "any → REOPENED on
    // regression"; AC-6), evaluated before any current claim because a
    // recurrence is a fact, not something a claim can override.
    if prior_status == Some(LifecycleStatus::VerifiedFixed) {
        return outcome(c, LifecycleStatus::Reopened, true);
    }

    // Rule 2 — explicit current-round WONT_FIX (recorded decision; retains
    // severity, still blocks via the unchanged verdict engine; not risk
    // acceptance; AC-7/AC-7a/AC-7d). Takes precedence over a fix claim, so a
    // recorded "won't fix" governs even if the evidence bar would pass (AC-7d
    // case 3) — the bar is not evaluated here.
    if cur == LifecycleStatus::WontFix {
        return outcome(c, LifecycleStatus::WontFix, false);
    }

    // Rule 3 — a fix is claimed: current FIX_CLAIMED, a current VERIFIED_FIXED
    // (claim-at-most — never self-accepted, AC-7c), or a carried FIX_CLAIMED.
    // Resolve against the §14/VR11 evidence bar: met ⇒ VERIFIED_FIXED, else the
    // claim stands as FIX_CLAIMED (verdict-equivalent to OPEN; AC-3/AC-4/AC-5).
    let claimed = matches!(
        cur,
        LifecycleStatus::FixClaimed | LifecycleStatus::VerifiedFixed
    ) || prior_status == Some(LifecycleStatus::FixClaimed);
    if claimed {
        let status = if meets_verified_fixed_bar(c, prior) {
            LifecycleStatus::VerifiedFixed
        } else {
            LifecycleStatus::FixClaimed
        };
        return outcome(c, status, false);
    }

    // Rule 4 — carried-over WONT_FIX (same weight as rule 2).
    if prior_status == Some(LifecycleStatus::WontFix) {
        return outcome(c, LifecycleStatus::WontFix, false);
    }

    // Rule 5 — carry-over / default. Carry a prior OPEN/REOPENED as recorded;
    // with no prior, a new finding defaults to OPEN (AC-1). FIX_CLAIMED/WONT_FIX
    // are never inferred here (§9.1/AC-7b).
    let status = match prior_status {
        Some(LifecycleStatus::Reopened) => LifecycleStatus::Reopened,
        // Open, or (defensively) any status not matched above, and the None case.
        _ => LifecycleStatus::Open,
    };
    outcome(c, status, false)
}

/// Build a [`LifecycleOutcome`] for `c`. `regression` is `true` only for the
/// rule-1 regression path (§14); every other rule passes `false`.
fn outcome(c: &Finding, lifecycle_status: LifecycleStatus, regression: bool) -> LifecycleOutcome {
    LifecycleOutcome {
        id: c.id.clone(),
        lifecycle_status,
        regression,
    }
}

/// The VERIFIED_FIXED evidence bar (§2.2 / §14), built as the conjunction of
/// mechanisms that already exist so this never diverges from the validator's
/// VERIFIED semantics:
///
/// 1. a `SUFFICIENT` support mapping on the current finding (same sufficiency
///    notion VR1 enforces for a VERIFIED finding), whose evidence is non-empty;
/// 2. the current finding's epistemic conclusion is `VERIFIED`;
/// 3. its `verification_criteria` is recorded (non-empty) — the fix is checked
///    against the finding's own criteria;
/// 4. **if the original property required execution**, the current VERIFIED
///    conclusion is itself execution-based (same VR11 predicate), so the fix is
///    held to the same execution bar the property required. "Requires execution"
///    is read via the VR11 predicate from the matched `prior` finding when one
///    exists, otherwise — for a first-time current-round claim with no prior —
///    from the current finding's own epistemic state (§9.3).
///
/// Missing / INSUFFICIENT / non-execution-where-required evidence fails the bar,
/// so absence is never a VERIFIED_FIXED (§9.4). A prior conclusion is never used
/// as evidence here (§15/C12), and a current `VERIFIED_FIXED`/`FIX_CLAIMED` is a
/// *claim*, not evidence — only `current`'s support/epistemic evidence is
/// weighed; `prior` contributes only the "requires execution" question (§9.3).
fn meets_verified_fixed_bar(current: &Finding, prior: Option<&Finding>) -> bool {
    // (1) current evidence is SUFFICIENT (and references at least one item).
    let has_sufficient = current
        .support_mappings
        .iter()
        .any(sufficient_with_evidence);
    if !has_sufficient {
        return false;
    }

    // (2) the current conclusion is VERIFIED.
    if current.epistemic_state.conclusion != EpistemicConclusion::Verified {
        return false;
    }

    // (3) the finding's own verification_criteria is recorded.
    if current.verification_criteria.trim().is_empty() {
        return false;
    }

    // (4) execution bar, reusing VR11's execution-based predicate. The property
    // that must be execution-verified is described by the prior finding when one
    // exists, else by the current finding itself (first-time claim, no prior).
    let property = prior.unwrap_or(current);
    if requires_execution(property) && !is_execution_based(&current.epistemic_state.facets) {
        return false;
    }

    true
}

/// A support mapping that is `SUFFICIENT` and actually references evidence (VR1
/// requires ≥1 evidence ref; the schema enforces non-emptiness structurally, and
/// we re-check here so the bar cannot pass on an empty mapping).
fn sufficient_with_evidence(m: &SupportMapping) -> bool {
    m.sufficiency == Sufficiency::Sufficient && !m.evidence_refs.is_empty()
}

/// Whether the property described by `finding` required execution-based evidence,
/// reusing VR11's definition (§9.3): a property is treated as execution-required
/// when the finding establishing it has an execution-based epistemic state —
/// i.e. a `VERIFIED` conclusion backed by an execution-based facet (OBSERVED).
/// This is the exact predicate VR11 uses to reject a static-only VERIFIED, so
/// Task 12 introduces no second notion of "requires execution". The caller
/// passes the matched prior finding when one exists, else the current finding.
fn requires_execution(finding: &Finding) -> bool {
    finding.epistemic_state.conclusion == EpistemicConclusion::Verified
        && is_execution_based(&finding.epistemic_state.facets)
}

/// VR11's execution-based-facet predicate: from the facet set alone, only
/// `OBSERVED` is unambiguously execution-based (see
/// [`crate::validation`] VR11). Reused verbatim here so the two never diverge.
fn is_execution_based(facets: &[Facet]) -> bool {
    facets.contains(&Facet::Observed)
}
