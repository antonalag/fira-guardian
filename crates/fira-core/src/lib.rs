//! # FIRA Guardian — CORE
//!
//! Platform-independent audit domain for FIRA Guardian, an independent,
//! evidence-based Release Readiness Auditor.
//!
//! This crate is platform-independent: it must not depend on `fira-policy`,
//! `fira-runtime`, `fira-adapters`, `fira-presentation`, or `fira-cli`, nor on
//! any host/OS capability. It owns the domain model, engines, and abstract
//! capability interfaces, and talks to the outside world only through those
//! interfaces. See `docs/contract/frozen-mvp-contract.md` (§1 layer ownership).

/// Frozen MVP Contract version this build targets.
///
/// Must equal the repository-root `CONTRACT_VERSION` file (asserted below).
pub const CONTRACT_VERSION: &str = "1.0-frozen+corr1-4";

/// Shared enums and id newtypes referenced across the domain model.
pub mod model;

/// AuditRequest (S1): invocation input.
pub mod request;

/// EpistemicState (S3).
pub mod epistemic;

/// Claim / Requirement / Invariant family (S6).
pub mod requirement;

/// Finding + FailureScenario + Confidence (S7).
pub mod finding;

/// Gate (S8).
pub mod gate;

/// CoverageStatement (S9).
pub mod coverage;

/// TechnicalAssessment + HumanDecision (S10).
pub mod assessment;

/// AuditReport (S11): the canonical machine report.
pub mod report;

/// EvidenceRef + SupportMapping (S4).
pub mod evidence;

/// ExecutionResult + VerificationMechanism (S5 / S12).
pub mod execution;

/// Abstract capability interfaces: RepositoryReader, CommandExecutor,
/// EvidenceCollector, AuditContextProvider.
///
/// CORE declares the trait shapes and calls only these four interfaces; it never
/// touches host FS or processes directly. The implementations (capability
/// enforcement, process execution, discovery, persistence) are RUNTIME/adapter
/// concerns, not CORE.
pub mod interfaces;

/// Validation-rule (VR) enforcement — the semantic / cross-reference portion of
/// VR1–VR13 (the structural rules live in the JSON Schemas under `schemas/`).
///
/// Every check is a pure computation over a parsed JSON value plus a small
/// `ValidationContext`; no host FS/process/network access.
pub mod validation;

/// Confidence rubric (CONF-1/CONF-2; E1/E5).
///
/// A pure function that derives a finding's confidence outcome from the
/// structured evidence model — never from the recorded `confidence` (CONF-1) —
/// plus the VR5 recompute-and-compare hook.
pub mod confidence;

/// Deterministic verdict engine (C10).
///
/// A pure function that computes the `TechnicalAssessment` from the structured
/// gates + findings — never from the recorded assessment (C10) — plus the VR7
/// recompute-and-compare hook.
pub mod verdict;

#[cfg(test)]
mod contract_version_test {
    use super::CONTRACT_VERSION;

    #[test]
    fn matches_root_marker_and_frozen_value() {
        let root = include_str!("../../../CONTRACT_VERSION");
        assert_eq!(root.trim(), CONTRACT_VERSION);
        assert_eq!(CONTRACT_VERSION, "1.0-frozen+corr1-4");
    }
}
