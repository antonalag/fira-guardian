//! `web_service` profile. §20 defines this class only "analogously / kept
//! small"; this matrix is the approved MVP POLICY decision. PersistenceDurability
//! stays recommended so the profile stays general for stateless services while
//! stateful systems route to `stateful_distributed`.

use fira_core::model::{GateName, ProfileId};

use super::{expectation, recommended, required, strings, AuditProfile, PROFILE_VERSION};

/// The `web_service` audit profile (approved MVP matrix).
pub(super) fn profile() -> AuditProfile {
    AuditProfile {
        profile_id: ProfileId::WebService,
        version: PROFILE_VERSION.to_string(),
        description: "Running request/response service exposed over a network.".to_string(),
        gates: vec![
            required(GateName::Build),
            required(GateName::Tests),
            required(GateName::Requirements),
            required(GateName::Correctness),
            required(GateName::Security),
            required(GateName::OperationalReadiness),
            recommended(GateName::Observability),
            recommended(GateName::FailureRecovery),
            recommended(GateName::Concurrency),
            recommended(GateName::PersistenceDurability),
            recommended(GateName::Documentation),
        ],
        focus_areas: strings(&[
            "authn/authz",
            "input validation",
            "error handling",
            "resource limits",
        ]),
        typical_failure_modes: strings(&[
            "injection",
            "unhandled errors leaking internals",
            "resource exhaustion under load",
        ]),
        minimum_evidence_expectations: vec![
            expectation(
                GateName::Security,
                "Security-relevant tests/mechanisms execute.",
            ),
            expectation(
                GateName::Tests,
                "Endpoint tests execute and PASS.",
            ),
            expectation(
                GateName::OperationalReadiness,
                "A startup/health mechanism is observed.",
            ),
        ],
    }
}
