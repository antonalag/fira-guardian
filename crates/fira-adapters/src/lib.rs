//! # FIRA Guardian — ADAPTERS
//!
//! Binds host environments to CORE's four capability interfaces. The CLI is the
//! first MVP adapter (Task 10); the Kiro adapter is the second (Task 13),
//! integrating with the Rust auditor over the invocation/transport boundary
//! (TRANSPORT-1). Adapters handle transport, host capability binding, persistence
//! location, and the human-confirmation UX — but never reinterpret CORE
//! semantics or alter the verdict.

/// CLI adapter wiring (first MVP adapter).
pub mod cli;

/// Adapter-side persistence wiring (Task 11, WS-1): the §17
/// `AuditContextProvider` implementor (renders via PRESENTATION, writes bytes via
/// the RUNTIME `WorkspaceSink`) and the `std::env` default-base resolver.
pub mod persistence;

/// Kiro adapter wiring (second MVP adapter). Implemented in Task 13.
pub mod kiro {}
