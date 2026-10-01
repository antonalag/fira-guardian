//! S6 Claim / Requirement / Invariant family.
//!
//! Verbatim translation of `schema-formalization.md` §S6: `DeclaredClaim`,
//! `InferredInvariant`, `ReleaseRequirement`, `SecurityProperty`. Task 4 types
//! the data only; it adds no business meaning (C3: no invented requirements) and
//! no validation beyond what the frozen schema fixes.

use serde::{Deserialize, Serialize};

use crate::model::{AlwaysTrue, GateName, IdRef, ProfileId, SecurityPropertySource};

/// DeclaredClaim (S6): a project-asserted claim (UntrustedClaims). Carries zero
/// evidentiary weight until independently verified (TRUST-1) — a runtime concern,
/// not a type-level one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredClaim {
    pub id: IdRef,
    pub statement: String,
    pub source_ref: String,
    pub security_relevant: bool,
}

/// InferredInvariant (S6): auditor-derived, code-internal correctness only.
/// `inferred` and `code_internal_only` are fixed to `true` by the schema
/// (`const: true`), typed as [`AlwaysTrue`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferredInvariant {
    pub id: IdRef,
    pub statement: String,
    pub derivation_evidence: Vec<IdRef>,
    pub inferred: AlwaysTrue,
    pub code_internal_only: AlwaysTrue,
}

/// ReleaseRequirement (S6): from a profile; can fail release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseRequirement {
    pub id: IdRef,
    pub statement: String,
    pub from_profile: ProfileId,
    pub gate: GateName,
}

/// SecurityProperty (S6): category overlay with a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityProperty {
    pub id: IdRef,
    pub statement: String,
    pub source: SecurityPropertySource,
    pub underlying_ref: String,
}
