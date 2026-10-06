//! S6 Claim / Requirement / Invariant family: `DeclaredClaim`,
//! `InferredInvariant`, `ReleaseRequirement`, `SecurityProperty`.

use serde::{Deserialize, Serialize};

use crate::model::{AlwaysTrue, GateName, IdRef, ProfileId, SecurityPropertySource};

/// DeclaredClaim (S6): a project-asserted claim. Carries zero evidentiary weight
/// until independently verified (TRUST-1), which is a runtime concern.
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
