//! S1 AuditRequest: the invocation input.
//!
//! Trust handling (TrustedInputs vs UntrustedClaims) is a runtime concern, not a
//! type-level one.

use serde::{Deserialize, Serialize};

use crate::model::{Capability, Depth, ProfileId};
use crate::requirement::DeclaredClaim;

/// AuditRequest (S1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRequest {
    /// Generated if absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_id: Option<String>,
    pub project_root: String,
    pub release_target: String,
    /// MVP = {READ, EXECUTE_EXISTING}.
    pub execution_boundary: Vec<Capability>,
    /// Project-asserted claims → UntrustedClaims.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_claims: Option<Vec<DeclaredClaim>>,
    /// If absent, Core infers and a human confirms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_profile: Option<ProfileId>,
    /// Re-audit input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_audit_report_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_depth: Option<Depth>,
}
