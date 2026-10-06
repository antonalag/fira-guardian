//! VR12 — `command_id` foreign-key resolution (EXEC-1).
//!
//! The legitimate ids come from
//! [`ValidationContext::known_mechanism_ids`](crate::validation::ValidationContext)
//! (the `declared_by_project=true` mechanisms). Any executed reference in
//! `coverage.executed_mechanisms[].command_id` that does not resolve to that set
//! is a violation — a synthesized command.

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
