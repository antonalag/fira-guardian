//! # FIRA Guardian — CORE
//!
//! Platform-independent audit domain for FIRA Guardian, an independent,
//! evidence-based Release Readiness Auditor.
//!
//! Per the Frozen MVP Contract (Layer ownership), this crate is
//! platform-independent: it must not depend on `fira-policy`, `fira-runtime`,
//! `fira-adapters`, `fira-presentation`, or `fira-cli`, nor on any host/OS
//! capability. It owns the domain model, engines, and abstract capability
//! interfaces — but talks to the outside world only through those interfaces.
//!
//! ## Task 1 scope
//! This file establishes the module *homes* that later tasks fill. No domain
//! logic, schemas-as-code, engines, or capability implementations exist yet;
//! each module is an empty home naming its owning contract section and task.

/// Frozen MVP Contract version this build targets.
///
/// Must equal the repository-root `CONTRACT_VERSION` file (asserted by
/// `contract_version_test`, AC-8).
pub const CONTRACT_VERSION: &str = "1.0-frozen+corr1-4";

// ---------------------------------------------------------------------------
// Module homes (empty in Task 1). Each names its schema and owning task.
// No fields, types with behavior, or logic are introduced here in Task 1.
// ---------------------------------------------------------------------------

/// EpistemicState (S3). Implemented in Task 4.
pub mod epistemic {}

/// Claim / Requirement / Invariant family (S6). Implemented in Task 4.
pub mod requirement {}

/// EvidenceRef + SupportMapping (S4). Implemented in Task 5.
pub mod evidence {}

/// ExecutionResult + VerificationMechanism (S5 / S12). Implemented in Task 5.
pub mod execution {}

/// Finding + FailureScenario + Confidence (S7). Implemented in Task 4 / 6.
pub mod finding {}

/// Gate (S8). Implemented in Task 4.
pub mod gate {}

/// CoverageStatement (S9). Implemented in Task 4.
pub mod coverage {}

/// TechnicalAssessment + HumanDecision (S10). Implemented in Task 4 / 7.
pub mod assessment {}

/// AuditReport (S11), including verification_mechanisms registry,
/// determinism_inputs_hash, divergence_records. Implemented in Task 4.
pub mod report {}

/// Abstract capability interfaces: RepositoryReader, CommandExecutor,
/// EvidenceCollector, AuditContextProvider. Implemented in Task 5 / 9.
///
/// CORE calls only these four interfaces; it never touches host FS or processes
/// directly. Project-tree capabilities are limited to `{READ, EXECUTE_EXISTING}`
/// (CAP-1); report persistence is a separate axis targeting the external audit
/// workspace (WS-1) and must never introduce a project `WRITE` capability.
pub mod interfaces {}

/// Validation-rule (VR) enforcement — the non-structural portion of VR1–VR13.
///
/// Task 2 home. Structural rules VR3/VR4/VR6/VR10 live in the JSON Schemas under
/// `schemas/`; this module owns the pure semantic / cross-reference checks
/// (VR1, VR2, VR8, VR9, VR11, VR12, VR13) plus the deferred VR5/VR7 recompute
/// hook. All checks are pure computations over parsed JSON values and a small
/// `ValidationContext`; no host FS/process/network access (Layer ownership).
pub mod validation;

#[cfg(test)]
mod contract_version_test {
    use super::CONTRACT_VERSION;

    /// AC-8: `fira-core::CONTRACT_VERSION` equals the repository-root
    /// `CONTRACT_VERSION` file and the frozen value `1.0-frozen+corr1-4`.
    #[test]
    fn matches_root_marker_and_frozen_value() {
        let root = include_str!("../../../CONTRACT_VERSION");
        assert_eq!(root.trim(), CONTRACT_VERSION);
        assert_eq!(CONTRACT_VERSION, "1.0-frozen+corr1-4");
    }
}
