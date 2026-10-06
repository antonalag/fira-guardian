//! S3 EpistemicState.
//!
//! The `conclusion=VERIFIED` ⇒ execution-based facet rule (E1/VR11) is not
//! encoded at the type level; it is enforced by the VR validator.

use serde::{Deserialize, Serialize};

use crate::model::{EpistemicConclusion, Facet};

/// EpistemicState (S3): a set of orthogonal facets plus a conclusion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpistemicState {
    /// Orthogonal facets (not a ladder). Modeled as a `Vec`; set-uniqueness is a
    /// schema concern, not enforced at the type level.
    pub facets: Vec<Facet>,
    pub conclusion: EpistemicConclusion,
}
