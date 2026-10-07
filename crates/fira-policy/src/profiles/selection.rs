//! Profile selection + confirm-gate projection (POLICY side of Task 9 P1).
//!
//! This is the `AuditProfile`-consuming half of P1: it turns a settled
//! `ProfileId` into an [`AuditProfile`] (via [`profile_for`]) and projects the
//! data the §12 P1 HUMAN CONFIRM GATE shows, plus a representation of the
//! applied selection that encodes PROF-1.
//!
//! It lives in POLICY (not CORE) because it consumes `AuditProfile`, a POLICY
//! type. It depends on CORE for `ProfileId` / `Classification` / the S11
//! `na_overrides` shape; no `CORE → POLICY` edge is created.

use fira_core::classification::Classification;
use fira_core::model::{GateName, ProfileId, RequirementLevel};
use fira_core::report::NaOverride;

use super::{profile_for, AuditProfile};

/// The data the HUMAN CONFIRM GATE displays (§12 P1): the class, the resulting
/// gate matrix, focus areas, typical failure modes, and the N/A justifications.
/// A pure projection of an [`AuditProfile`]; it introduces no new semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmGateView {
    pub profile_id: ProfileId,
    pub version: String,
    pub gate_matrix: Vec<(GateName, RequirementLevel)>,
    pub focus_areas: Vec<String>,
    pub typical_failure_modes: Vec<String>,
    pub na_justifications: Vec<(GateName, String)>,
}

/// Project a resolved profile into the confirm-gate view.
pub fn confirm_gate_view(profile: &AuditProfile) -> ConfirmGateView {
    let gate_matrix = profile
        .gates
        .iter()
        .map(|g| (g.gate, g.requirement_level))
        .collect();

    let na_justifications = profile
        .gates
        .iter()
        .filter(|g| g.requirement_level == RequirementLevel::NotApplicable)
        .filter_map(|g| g.na_justification.clone().map(|j| (g.gate, j)))
        .collect();

    ConfirmGateView {
        profile_id: profile.profile_id,
        version: profile.version.clone(),
        gate_matrix,
        focus_areas: profile.focus_areas.clone(),
        typical_failure_modes: profile.typical_failure_modes.clone(),
        na_justifications,
    }
}

/// Convenience: resolve a `ProfileId` to its profile and project the view.
pub fn confirm_gate_view_for(id: ProfileId) -> ConfirmGateView {
    confirm_gate_view(&profile_for(id))
}

/// The applied P1 selection: the classifier proposal plus the class the human
/// confirmed and any human N/A overrides.
///
/// PROF-1 is enforced structurally: `applied` is the closed [`ProfileId`] enum
/// (so a sixth/custom profile is unrepresentable), and `na_overrides` carries
/// only the S11 `{gate, by:human}` shape (so a human may mark a gate N/A but
/// cannot otherwise change a gate's requirement level or redefine gate
/// semantics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedSelection {
    /// What the classifier proposed (may be `Undetermined`).
    pub proposal: Classification,
    /// The class the human applied — one of the five `ProfileId` values
    /// (confirm = proposal, or reclassify among the five).
    pub applied: ProfileId,
    /// Human N/A overrides, each tagged `by: human`.
    pub na_overrides: Vec<NaOverride>,
}

impl AppliedSelection {
    /// The human confirmed the classifier's proposed class unchanged.
    ///
    /// Returns `None` when the proposal is `Undetermined` (there is no proposed
    /// class to confirm — the human must choose via [`AppliedSelection::reclassified`]).
    pub fn confirm(proposal: Classification) -> Option<Self> {
        match &proposal {
            Classification::Classified { profile_id, .. } => {
                let applied = *profile_id;
                Some(AppliedSelection {
                    proposal,
                    applied,
                    na_overrides: Vec::new(),
                })
            }
            Classification::Undetermined { .. } => None,
        }
    }

    /// The human applied a class (either confirming the proposal or choosing a
    /// different one of the five). Valid for any proposal, including
    /// `Undetermined`.
    pub fn reclassified(proposal: Classification, applied: ProfileId) -> Self {
        AppliedSelection {
            proposal,
            applied,
            na_overrides: Vec::new(),
        }
    }

    /// Record a human N/A override on the applied selection.
    pub fn with_na_override(mut self, over: NaOverride) -> Self {
        self.na_overrides.push(over);
        self
    }

    /// The resolved applied profile (POLICY data for the applied class).
    pub fn applied_profile(&self) -> AuditProfile {
        profile_for(self.applied)
    }
}
