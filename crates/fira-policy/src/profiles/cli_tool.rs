//! `cli_tool` profile. §20 defines this class only "analogously / kept small";
//! this matrix is the approved MVP POLICY decision (incl. the approved
//! correction: Security required, Concurrency recommended, PersistenceDurability
//! and OperationalReadiness N/A on semantic grounds).

use fira_core::model::{GateName, ProfileId};

use super::{
    expectation, not_applicable, recommended, required, strings, AuditProfile, PROFILE_VERSION,
};

/// The `cli_tool` audit profile (approved MVP matrix).
pub(super) fn profile() -> AuditProfile {
    AuditProfile {
        profile_id: ProfileId::CliTool,
        version: PROFILE_VERSION.to_string(),
        description: "User-run command-line tool invoked to perform a task and exit.".to_string(),
        gates: vec![
            required(GateName::Build),
            required(GateName::Tests),
            required(GateName::Requirements),
            required(GateName::Correctness),
            required(GateName::Security),
            recommended(GateName::Documentation),
            recommended(GateName::FailureRecovery),
            recommended(GateName::Observability),
            recommended(GateName::Concurrency),
            not_applicable(
                GateName::PersistenceDurability,
                "Durable persistence is not inherent to the command-line-tool class.",
            ),
            not_applicable(
                GateName::OperationalReadiness,
                "This class is a user-run tool, not a long-running operational service.",
            ),
        ],
        focus_areas: strings(&[
            "argument/exit-code correctness",
            "input handling",
            "helpful errors",
        ]),
        typical_failure_modes: strings(&[
            "wrong exit codes",
            "crashes on malformed input",
            "silent failure",
        ]),
        minimum_evidence_expectations: vec![
            expectation(
                GateName::Tests,
                "CLI behavior tests execute and PASS.",
            ),
            expectation(
                GateName::Build,
                "The build produces a runnable artifact.",
            ),
        ],
    }
}
