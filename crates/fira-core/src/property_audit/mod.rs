//! Single-property semantic audit: the `JournalStore` recovery-generation
//! **fencing invariant** (one targeted property; vertical slice).
//!
//! This is FIRA's first step beyond "a verification command passed": it audits
//! whether there is **independent evidence** for one important,
//! structurally-declared property, classifying that evidence on the §3 facet
//! ladder — and, critically, it does **not** treat a passing test suite as proof
//! that the specific property holds.
//!
//! # The epistemic boundary (the point of this slice)
//!
//! Evidence is classified into the frozen `{SPECIFIED, IMPLEMENTED, TESTED,
//! OBSERVED}` facets, and a `VERIFIED` conclusion requires an execution-based
//! `OBSERVED` facet (C6/VR11). The five distinct levels are:
//!
//! 1. **SPECIFIED** — the invariant is declared in source.
//! 2. **IMPLEMENTED** — the implementation contains the generation/CAS guard.
//! 3. **TESTED** — a test body appears to exercise stale-generation rejection
//!    (static reading of test code; TESTED-not-executed).
//! 4. **mechanism OBSERVED** — the test *mechanism* executed and PASSED. This is
//!    an observed fact about the *mechanism*, **not** about this property.
//! 5. **property OBSERVED ⇒ VERIFIED** — the specific fencing behavior was shown
//!    to have been exercised by that execution.
//!
//! **Option A (this MVP):** FIRA cannot, under {READ, EXECUTE_EXISTING} without
//! captured-output parsing, independently demonstrate level 5. Therefore the
//! property's **ceiling is UNVERIFIED** even when levels 1–4 all hold and the
//! `npm-test` mechanism PASSED. That is intentional and correct: a passing suite
//! plus the mere existence of a fencing test is **not** proof the property was
//! exercised. This module never manufactures level 5 from level 4 (the forbidden
//! inference), and it is passed no grounded exercised-evidence by the Option-A
//! caller, so it never emits a `VERIFIED` conclusion in this slice.
//!
//! This module is pure CORE: it performs no I/O. The adapter reads the targeted
//! source (via the READ capability) and the executed mechanism's outcome, and
//! hands this module the resulting evidence inputs; the module classifies them.

use crate::epistemic::EpistemicState;
use crate::model::{EpistemicConclusion, Facet, GateName};

/// The property this slice audits (fixed, single target — not a generic
/// extractor). Named so the record/rationale can reference it precisely.
pub const FENCING_PROPERTY_TITLE: &str =
    "JournalStore writes are fenced by recovery_generation (a superseded owner's \
     writes are rejected)";

/// The single gate this property maps to (§9.2 ruling): recovery safety.
pub const FENCING_GATE: GateName = GateName::FailureRecovery;

/// Whether the specific fencing behavior was independently shown to have been
/// **exercised** by an executed mechanism (level 5). Under Option A this is
/// always `NotEstablished` — the caller has no grounded way to demonstrate it,
/// and this module must therefore never conclude VERIFIED. The variant exists so
/// the type honestly represents the boundary (and so a future, *grounded*
/// extension could supply `Exercised` from real evidence — not in this slice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyExercised {
    /// FIRA could not independently establish that the behavior was exercised.
    /// (Option A always uses this; a green suite does NOT upgrade it.)
    NotEstablished,
    /// Reserved: the behavior was shown exercised by real execution evidence.
    /// Not produced in this slice (Option A); present only so the boundary is
    /// explicit and the type total.
    Exercised,
}

/// The evidence inputs the adapter gathers under {READ, EXECUTE_EXISTING}. Each
/// static flag is a conservative presence signal over already-read source text;
/// `mechanism_passed` is the executed test mechanism's PASSED fact (level 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FencingEvidence {
    /// Level 1: the fencing contract is declared in the store interface.
    pub specified_in_interface: bool,
    /// Level 2: the implementation contains the recovery_generation/CAS guard.
    pub implemented_guard_present: bool,
    /// Level 3: a test body appears to assert stale-generation rejection.
    pub tested_rejection_present: bool,
    /// Level 4: the test *mechanism* executed and PASSED (mechanism OBSERVED).
    pub mechanism_passed: bool,
    /// Level 5: whether the property's specific behavior was shown exercised.
    /// Always `NotEstablished` under Option A.
    pub exercised: PropertyExercised,
}

/// The classification outcome for the fencing property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FencingAssessment {
    /// The epistemic state (facets + conclusion) on the §3 ladder.
    pub epistemic_state: EpistemicState,
    /// A human-readable rationale naming exactly which levels were established
    /// and why the conclusion is what it is (honest, evidence-scoped).
    pub rationale: String,
}

impl FencingAssessment {
    /// True iff the conclusion is VERIFIED (never true under Option A inputs).
    pub fn is_verified(&self) -> bool {
        self.epistemic_state.conclusion == EpistemicConclusion::Verified
    }
}

/// Classify the fencing property from gathered evidence (pure; deterministic).
///
/// Facets accumulate per the levels that hold. The conclusion is `VERIFIED`
/// **iff** an execution-based `OBSERVED` facet is present — which requires
/// `exercised == Exercised` (level 5). `mechanism_passed` alone (level 4) adds
/// **no** facet for the property: a passing suite is not evidence that this
/// property was exercised (the forbidden inference). Everything short of a
/// grounded level-5 is `UNVERIFIED` (C5/C6/VR11).
pub fn classify_fencing(evidence: &FencingEvidence) -> FencingAssessment {
    let mut facets: Vec<Facet> = Vec::new();
    if evidence.specified_in_interface {
        facets.push(Facet::Specified);
    }
    if evidence.implemented_guard_present {
        facets.push(Facet::Implemented);
    }
    if evidence.tested_rejection_present {
        facets.push(Facet::Tested);
    }
    // Level 5 is the ONLY path to an OBSERVED facet for the property. A passing
    // mechanism (level 4) is deliberately NOT sufficient — it says nothing about
    // whether *this* behavior was exercised.
    let property_observed = matches!(evidence.exercised, PropertyExercised::Exercised);
    if property_observed {
        facets.push(Facet::Observed);
    }

    // VERIFIED iff an execution-based (OBSERVED) facet is present (VR11); else
    // UNVERIFIED — the honest ceiling under Option A.
    let conclusion = if property_observed {
        EpistemicConclusion::Verified
    } else {
        EpistemicConclusion::Unverified
    };

    let rationale = build_rationale(evidence, conclusion);

    FencingAssessment {
        epistemic_state: EpistemicState { facets, conclusion },
        rationale,
    }
}

fn build_rationale(evidence: &FencingEvidence, conclusion: EpistemicConclusion) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if evidence.specified_in_interface {
        parts.push("declared in the store interface (SPECIFIED)");
    }
    if evidence.implemented_guard_present {
        parts.push("a recovery_generation guard is present in the implementation (IMPLEMENTED)");
    }
    if evidence.tested_rejection_present {
        parts.push("a test body appears to assert stale-generation rejection (TESTED)");
    }
    if evidence.mechanism_passed {
        parts.push("the declared test mechanism executed and passed (mechanism OBSERVED)");
    }
    let found = if parts.is_empty() {
        "no fencing evidence was found".to_string()
    } else {
        parts.join("; ")
    };

    match conclusion {
        EpistemicConclusion::Verified => format!(
            "VERIFIED: {found}; and the specific fencing behavior was independently \
             shown to have been exercised by that execution."
        ),
        EpistemicConclusion::Unverified => format!(
            "UNVERIFIED: {found}. FIRA did not independently demonstrate that the \
             specific fencing behavior was exercised by the executed mechanism, so a \
             passing test suite does not establish this property (lack of evidence is \
             not PASS)."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(spec: bool, impl_: bool, tested: bool, passed: bool) -> FencingEvidence {
        FencingEvidence {
            specified_in_interface: spec,
            implemented_guard_present: impl_,
            tested_rejection_present: tested,
            mechanism_passed: passed,
            exercised: PropertyExercised::NotEstablished,
        }
    }

    #[test]
    fn full_static_plus_passing_suite_is_still_unverified() {
        // Levels 1–4 all hold (and the suite PASSED), but with no grounded
        // exercised evidence the property is UNVERIFIED — the forbidden
        // inference is rejected.
        let a = classify_fencing(&ev(true, true, true, true));
        assert_eq!(
            a.epistemic_state.conclusion,
            EpistemicConclusion::Unverified
        );
        assert!(!a.is_verified());
        assert!(a.epistemic_state.facets.contains(&Facet::Specified));
        assert!(a.epistemic_state.facets.contains(&Facet::Implemented));
        assert!(a.epistemic_state.facets.contains(&Facet::Tested));
        // No OBSERVED facet for the property from a mere passing mechanism.
        assert!(!a.epistemic_state.facets.contains(&Facet::Observed));
    }

    #[test]
    fn verified_only_with_grounded_exercised_evidence() {
        // The ONLY way to VERIFIED: a grounded level-5. (Not produced in Option
        // A; asserted here to pin the boundary semantics.)
        let mut e = ev(true, true, true, true);
        e.exercised = PropertyExercised::Exercised;
        let a = classify_fencing(&e);
        assert_eq!(a.epistemic_state.conclusion, EpistemicConclusion::Verified);
        assert!(a.epistemic_state.facets.contains(&Facet::Observed));
    }

    #[test]
    fn missing_implementation_drops_the_implemented_facet() {
        let a = classify_fencing(&ev(true, false, true, true));
        assert!(!a.epistemic_state.facets.contains(&Facet::Implemented));
        assert_eq!(
            a.epistemic_state.conclusion,
            EpistemicConclusion::Unverified
        );
    }

    #[test]
    fn deterministic() {
        let e = ev(true, true, true, true);
        assert_eq!(classify_fencing(&e), classify_fencing(&e));
    }
}
