//! S3 EpistemicState.
//!
//! Verbatim translation of `schema-formalization.md` §S3. Facets form a set
//! (the schema's `uniqueItems`); the `conclusion=VERIFIED` ⇒ execution-based
//! facet rule (E1/VR11) is **not** encoded here — it is already enforced by the
//! Task 2 VR validator. Task 4 types the data only.

use serde::{Deserialize, Serialize};

use crate::model::{EpistemicConclusion, Facet};

/// EpistemicState (S3): a set of orthogonal facets plus a conclusion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpistemicState {
    /// Orthogonal facets (not a ladder). Modeled as a `Vec`; set-uniqueness is
    /// the schema's `uniqueItems` concern, not enforced at the type level.
    pub facets: Vec<Facet>,
    pub conclusion: EpistemicConclusion,
}
