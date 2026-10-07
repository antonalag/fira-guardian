//! `library` profile. Gate matrix fixed verbatim by frozen contract §20.

use fira_core::model::{GateName, ProfileId};

use super::{
    expectation, not_applicable, recommended, required, strings, AuditProfile, PROFILE_VERSION,
};

/// The `library` audit profile (§20 reference matrix).
pub(super) fn profile() -> AuditProfile {
    AuditProfile {
        profile_id: ProfileId::Library,
        version: PROFILE_VERSION.to_string(),
        description: "Reusable library consumed by other code; no running process or stored state of its own.".to_string(),
        gates: vec![
            required(GateName::Build),
            required(GateName::Tests),
            required(GateName::Requirements),
            required(GateName::Correctness),
            required(GateName::Documentation),
            recommended(GateName::Security),
            recommended(GateName::Concurrency),
            recommended(GateName::FailureRecovery),
            not_applicable(
                GateName::PersistenceDurability,
                "A library owns no persistent state of its own.",
            ),
            not_applicable(
                GateName::Observability,
                "A library is not a running process with an observability surface.",
            ),
            not_applicable(
                GateName::OperationalReadiness,
                "A library is not an operational service.",
            ),
        ],
        focus_areas: strings(&[
            "public API stability",
            "input validation",
            "backward compatibility",
        ]),
        typical_failure_modes: strings(&[
            "unhandled edge-case inputs",
            "breaking API changes",
        ]),
        minimum_evidence_expectations: vec![
            expectation(GateName::Tests, "Existing unit tests execute and PASS."),
            expectation(GateName::Build, "The build mechanism runs clean."),
        ],
    }
}
