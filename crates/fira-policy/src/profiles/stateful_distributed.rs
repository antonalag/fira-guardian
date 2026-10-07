//! `stateful_distributed` profile. Gate matrix fixed verbatim by frozen
//! contract §20.

use fira_core::model::{GateName, ProfileId};

use super::{expectation, recommended, required, strings, AuditProfile, PROFILE_VERSION};

/// The `stateful_distributed` audit profile (§20 reference matrix).
pub(super) fn profile() -> AuditProfile {
    AuditProfile {
        profile_id: ProfileId::StatefulDistributed,
        version: PROFILE_VERSION.to_string(),
        description: "Stateful, distributed system holding durable state and coordinating across nodes.".to_string(),
        gates: vec![
            required(GateName::Build),
            required(GateName::Tests),
            required(GateName::Requirements),
            required(GateName::Correctness),
            required(GateName::FailureRecovery),
            required(GateName::PersistenceDurability),
            required(GateName::Concurrency),
            required(GateName::Security),
            required(GateName::OperationalReadiness),
            recommended(GateName::Observability),
            recommended(GateName::Documentation),
        ],
        focus_areas: strings(&[
            "crash/restart recovery",
            "data durability",
            "concurrency safety",
            "partition behavior",
        ]),
        typical_failure_modes: strings(&[
            "lost writes on crash",
            "race conditions",
            "split-brain",
            "unbounded queues",
        ]),
        minimum_evidence_expectations: vec![
            expectation(
                GateName::FailureRecovery,
                "A failure case is executed and recovery observed.",
            ),
            expectation(
                GateName::PersistenceDurability,
                "Durability is exercised across a restart.",
            ),
            expectation(
                GateName::Concurrency,
                "A concurrent scenario is executed.",
            ),
        ],
    }
}
