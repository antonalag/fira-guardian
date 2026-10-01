//! # FIRA Guardian — ADAPTERS
//!
//! Binds host environments to CORE's four capability interfaces. The CLI is the
//! first MVP adapter (Task 10); the Kiro adapter is the second (Task 13),
//! integrating with the Rust auditor over the invocation/transport boundary
//! (TRANSPORT-1). Adapters handle transport, host capability binding, persistence
//! location, and the human-confirmation UX — but never reinterpret CORE
//! semantics or alter the verdict.
//!
//! ## Task 1 scope
//! Home only. No adapter behavior is implemented yet.

/// CLI adapter wiring (first MVP adapter). Implemented in Task 10.
pub mod cli {}

/// Kiro adapter wiring (second MVP adapter). Implemented in Task 13.
pub mod kiro {}
