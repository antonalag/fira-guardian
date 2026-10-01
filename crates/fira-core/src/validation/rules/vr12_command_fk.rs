//! VR12 — command_id foreign-key resolution. *(correction 3)*
//!
//! From `schema-formalization.md` / frozen contract §6 EXEC-1: every
//! `ExecutionResult.command_id` and executed reference resolves to a
//! VerificationMechanism with `declared_by_project=true`; synthesized commands
//! are rejected.
//!
//! ## Mechanical check
//!
//! The set of legitimate ids is supplied by
//! [`ValidationContext::known_mechanism_ids`](crate::validation::ValidationContext),
//! which a caller derives from the report's `verification_mechanisms` where
//! `declared_by_project=true` (see `StaticValidationContext::from_report`). Any
//! executed reference in `coverage.executed_mechanisms[].command_id` that does
//! not resolve to that set is a VR12 violation. This is a deterministic FK
//! resolution with no new semantics.

use serde_json::Value;

use crate::validation::{ValidationContext, Violation, Vr};

/// Collect executed command references from the report's coverage statement.
fn executed_command_ids(report: &Value) -> Vec<(String, String)> {
    let mut refs = Vec::new();
    if let Some(execs) = report
        .get("coverage")
        .and_then(|c| c.get("executed_mechanisms"))
        .and_then(Value::as_array)
    {
        for (i, e) in execs.iter().enumerate() {
            if let Some(id) = e.get("command_id").and_then(Value::as_str) {
                refs.push((format!("coverage.executed_mechanisms[{i}].command_id"), id.to_string()));
            }
        }
    }
    refs
}

pub(crate) fn check(report: &Value, ctx: &dyn ValidationContext) -> Vec<Violation> {
    let mut out = Vec::new();
    let known = ctx.known_mechanism_ids();
    for (locator, id) in executed_command_ids(report) {
        if !known.iter().any(|k| k == &id) {
            out.push(Violation::new(
                Vr::Vr12,
                locator,
                format!(
                    "executed command_id `{id}` does not resolve to a declared_by_project=true VerificationMechanism (synthesized command rejected)"
                ),
            ));
        }
    }
    out
}
