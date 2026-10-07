//! # FIRA Guardian — POLICY
//!
//! Versioned policy data consumed by CORE: the five built-in audit profiles
//! (`library`, `cli_tool`, `web_service`, `stateful_distributed`,
//! `batch_pipeline`). Per Layer ownership, POLICY depends at most on `fira-core`.
//!
//! Profiles are data, not behavior: they set per-gate requirement levels,
//! focus areas, typical failure modes, and minimum-evidence expectations. They
//! never define gate evaluation (§7) or verdict computation (§11, Task 7).
//!
//! The frozen severity enum (S7) and the parameter-free confidence rubric
//! (Task 6) satisfy §1's other POLICY phrases; this crate introduces no severity
//! or confidence parameters.

/// Built-in audit profiles (S2).
pub mod profiles;

pub use profiles::{
    all, profile_for, AuditProfile, MinimumEvidenceExpectation, ProfileGate, ALL_IDS,
    PROFILE_VERSION,
};
