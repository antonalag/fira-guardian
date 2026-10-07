//! # FIRA Guardian — PRESENTATION
//!
//! Renders the canonical machine-readable `AuditReport` into human/consumer
//! formats (JSON, Markdown). The machine report remains the single source of
//! truth; presentation never adjudicates readiness and never mutates the report.
//!
/// JSON / Markdown rendering from `AuditReport`.
pub mod render;

pub use render::{render_json, render_markdown};
