//! # FIRA Guardian — PRESENTATION
//!
//! Renders the canonical machine-readable `AuditReport` into human/consumer
//! formats (JSON, Markdown). The machine report remains the single source of
//! truth; presentation never adjudicates readiness and never mutates the report.
//!
//! ## Task 1 scope
//! Home only. Rendering is implemented alongside the CLI adapter (Task 10),
//! deriving JSON and Markdown from one report.

/// JSON / Markdown rendering from `AuditReport`. Implemented in Task 10.
pub mod render {}
