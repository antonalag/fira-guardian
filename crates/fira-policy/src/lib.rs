//! # FIRA Guardian — POLICY
//!
//! Versioned policy data consumed by CORE: the five built-in audit profiles
//! (`library`, `cli_tool`, `web_service`, `stateful_distributed`,
//! `batch_pipeline`), the severity model, confidence-rubric parameters, and
//! release-gate rules (Frozen MVP Contract: Profiles, PROF-1).
//!
//! Per Layer ownership, POLICY depends at most on `fira-core`.
//!
//! ## Task 1 scope
//! Home only. No profiles, rubric parameters, or rules are implemented yet;
//! that is Task 8 (profiles + classifier) and Task 6 (rubric parameters).

/// Built-in audit profiles (S2). Implemented in Task 8.
pub mod profiles {}
