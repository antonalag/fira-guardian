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
//! ## Scope note
//! Task 1 established the module *homes*. Task 2 added the `validation` module
//! (VR validator over JSON values). Task 4 fills the domain-model homes it owns
//! (S1/S3/S6/S7/S8/S9/S10/S11) with `serde`-(de)serializable Rust types, plus a
//! shared [`model`] module of frozen enums/newtypes. The confidence rubric
//! (Task 6), verdict engine (Task 7), the final S4/S5/S12 types and capability
//! interfaces (Task 5), and the S2 profile data (Task 8) are **not** here.

/// Frozen MVP Contract version this build targets.
///
/// Must equal the repository-root `CONTRACT_VERSION` file (asserted by
/// `contract_version_test`, AC-8).
pub const CONTRACT_VERSION: &str = "1.0-frozen+corr1-4";

// ---------------------------------------------------------------------------
// Shared model building blocks (Task 4): frozen enums + id newtypes.
// ---------------------------------------------------------------------------

/// Shared enums and id newtypes referenced across the domain model (Task 4).
pub mod model;

// ---------------------------------------------------------------------------
// CORE domain model (Task 4-owned schemas: S1, S3, S6, S7, S8, S9, S10, S11).
// ---------------------------------------------------------------------------

/// AuditRequest (S1). Invocation input (frozen contract §18).
pub mod request;

/// EpistemicState (S3). Implemented in Task 4.
pub mod epistemic;

/// Claim / Requirement / Invariant family (S6). Implemented in Task 4.
pub mod requirement;

/// Finding + FailureScenario + Confidence (S7). Types in Task 4; the confidence
/// rubric (VR5) is Task 6 — `confidence`/`confidence_derivation` are plain data.
pub mod finding;

/// Gate (S8). Implemented in Task 4.
pub mod gate;

/// CoverageStatement (S9). Implemented in Task 4.
pub mod coverage;

/// TechnicalAssessment + HumanDecision (S10). Types in Task 4; the verdict
/// engine (VR7) is Task 7 — `computed_by_rule` is plain data.
pub mod assessment;

/// AuditReport (S11), including the verification_mechanisms registry,
/// determinism_inputs_hash, and divergence_records. Implemented in Task 4.
pub mod report;

// ---------------------------------------------------------------------------
// S4 / S5 / S12 domain types (Task 5). The Task 4 provisional seams
// (SupportMapping, VerificationMechanism) are promoted in place here; EvidenceRef
// and ExecutionResult are added. Data types only — no VR logic, no execution or
// evidence-collection behavior (that is RUNTIME via `interfaces`, Task 9/10/11).
// ---------------------------------------------------------------------------

/// EvidenceRef + SupportMapping (S4). Final (Task 5).
pub mod evidence;

/// ExecutionResult + VerificationMechanism (S5 / S12). Final (Task 5).
pub mod execution;

/// Abstract capability interfaces: RepositoryReader, CommandExecutor,
/// EvidenceCollector, AuditContextProvider (frozen contract §17). Task 5
/// declares the **abstract trait shapes**; CORE calls only these four interfaces
/// and never touches host FS or processes directly. Project-tree capabilities
/// are limited to `{READ, EXECUTE_EXISTING}` (CAP-1); report persistence is a
/// separate axis targeting the external audit workspace (WS-1) and must never
/// introduce a project `WRITE` capability. The trait **implementations**
/// (enforcement, process execution, discovery, persistence) are RUNTIME/adapter
/// concerns — Task 9/10/11 — not CORE.
pub mod interfaces;

/// Validation-rule (VR) enforcement — the non-structural portion of VR1–VR13.
///
/// Task 2 home. Structural rules VR3/VR4/VR6/VR10 live in the JSON Schemas under
/// `schemas/`; this module owns the pure semantic / cross-reference checks
/// (VR1, VR2, VR8, VR9, VR11, VR12, VR13) plus the deferred VR5/VR7 recompute
/// hook. All checks are pure computations over parsed JSON values and a small
/// `ValidationContext`; no host FS/process/network access (Layer ownership).
pub mod validation;

/// Confidence rubric (frozen contract §8; CONF-1/CONF-2; E1/E5). Task 6.
///
/// A pure, deterministic CORE function that derives a finding's confidence
/// outcome from the structured evidence model (never from the recorded
/// `confidence`, CONF-1), plus the VR5 recompute-and-compare hook. Only the
/// contract-fixed, deterministic portion is implemented; the qualitative
/// MEDIUM/LOW distinction is intentionally not mechanized (no structured signal
/// for it). No rubric parameters are introduced (those are POLICY). No host
/// FS/process/network. The verdict engine (VR7) is Task 7.
pub mod confidence;

/// Deterministic verdict engine (frozen contract §11; C10). Task 7.
///
/// A pure, deterministic CORE function that computes the `TechnicalAssessment`
/// (result + blocking/risk sets + the FIRA-set `HumanDecision`) from the
/// structured gates + findings (never from the recorded assessment, C10), per
/// §11 Steps 1–4, plus the VR7 recompute-and-compare hook. Gate states are
/// consumed as recorded (§7 gate-state derivation is a separate concern);
/// `minimum_evidence_expectations` are POLICY (Task 8) and are supplied via a
/// caller input defaulting to empty. No host FS/process/network. The confidence
/// rubric (VR5) is Task 6.
pub mod verdict;

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
