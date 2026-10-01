//! VR2 — failure-scenario anchor for failure-class findings.
//!
//! From `schema-formalization.md`: a concurrency/durability/recovery finding ⇒
//! `failure_scenario.anchor` non-null. The `failure_scenario` object shape
//! (anchor/trigger/consequence, with anchor required *within* the object) is
//! structural; whether a finding *is* a failure-class finding — and therefore
//! must carry a `failure_scenario` at all — is the semantic coupling enforced
//! here.
//!
//! A finding is treated as failure-class (mechanically, no new taxonomy) when
//! its `category` contains "concurrency", "durability", or "recovery" (case-
//! insensitive), or when `gates_affected` includes one of the corresponding
//! gate names (Concurrency, PersistenceDurability, FailureRecovery — the frozen
//! contract §20 gates that own those classes). For such a finding,
//! `failure_scenario.anchor` must be present and non-empty.

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
