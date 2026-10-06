//! VR2 — a concurrency/durability/recovery finding requires a non-null
//! `failure_scenario.anchor`.
//!
//! The anchor-within-the-object requirement is structural; what this rule adds
//! is deciding whether a finding *is* failure-class: its `category` contains
//! "concurrency"/"durability"/"recovery" (case-insensitive), or `gates_affected`
//! includes Concurrency / PersistenceDurability / FailureRecovery. The same
//! heuristic is mirrored in the confidence rubric so the classification stays
//! consistent.

use serde_json::Value;

use crate::validation::rules::findings;
use crate::validation::{Violation, Vr};

/// Gate names (frozen contract §20) that denote the failure classes VR2 covers.
const FAILURE_CLASS_GATES: [&str; 3] = ["Concurrency", "PersistenceDurability", "FailureRecovery"];

/// Lowercase category tokens that denote the failure classes VR2 covers.
const FAILURE_CLASS_TOKENS: [&str; 3] = ["concurrency", "durability", "recovery"];

fn is_failure_class(finding: &Value) -> bool {
    let category_hit = finding
        .get("category")
        .and_then(Value::as_str)
        .map(|c| {
            let lower = c.to_ascii_lowercase();
            FAILURE_CLASS_TOKENS.iter().any(|t| lower.contains(t))
        })
        .unwrap_or(false);

    let gate_hit = finding
        .get("gates_affected")
        .and_then(Value::as_array)
        .map(|gs| {
            gs.iter()
                .filter_map(Value::as_str)
                .any(|g| FAILURE_CLASS_GATES.contains(&g))
        })
        .unwrap_or(false);

    category_hit || gate_hit
}

fn has_nonempty_anchor(finding: &Value) -> bool {
    finding
        .get("failure_scenario")
        .and_then(|fs| fs.get("anchor"))
        .and_then(Value::as_str)
        .map(|a| !a.is_empty())
        .unwrap_or(false)
}

pub(crate) fn check(report: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    for (i, finding) in findings(report) {
        if is_failure_class(finding) && !has_nonempty_anchor(finding) {
            out.push(Violation::new(
                Vr::Vr2,
                format!("findings[{i}]"),
                "concurrency/durability/recovery finding is missing failure_scenario.anchor",
            ));
        }
    }
    out
}
