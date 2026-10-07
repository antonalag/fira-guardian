//! `batch_pipeline` profile. §20 defines this class only "analogously / kept
//! small"; this matrix is the approved MVP POLICY decision. `Documentation` is
//! N/A in the sense that it does not gate release for this class — it may still
//! be inspected and reported.

use fira_core::model::{GateName, ProfileId};

use super::{
    expectation, not_applicable, recommended, required, strings, AuditProfile, PROFILE_VERSION,
};

/// The `batch_pipeline` audit profile (approved MVP matrix).
pub(super) fn profile() -> AuditProfile {
    AuditProfile {
        profile_id: ProfileId::BatchPipeline,
        version: PROFILE_VERSION.to_string(),
        description: "Scheduled or triggered data-processing job that runs to completion.".to_string(),
        gates: vec![
            required(GateName::Build),
            required(GateName::Tests),
            required(GateName::Requirements),
            required(GateName::Correctness),
            required(GateName::FailureRecovery),
            required(GateName::PersistenceDurability),
            recommended(GateName::Observability),
            recommended(GateName::OperationalReadiness),
            recommended(GateName::Concurrency),
            recommended(GateName::Security),
            not_applicable(
                GateName::Documentation,
                "Documentation is not a release gate for this class; it may still be inspected and reported, but does not gate release.",
            ),
        ],
        focus_areas: strings(&[
            "idempotency",
            "partial-failure recovery",
            "output integrity",
        ]),
        typical_failure_modes: strings(&[
            "partial writes on failure",
            "duplicate processing",
            "silent data loss",
        ]),
        minimum_evidence_expectations: vec![
            expectation(
                GateName::FailureRecovery,
                "A failed-run resume path is executed.",
            ),
            expectation(
                GateName::PersistenceDurability,
                "Output durability is exercised.",
            ),
            expectation(
                GateName::Tests,
                "Transform tests execute and PASS.",
            ),
        ],
    }
}
