//! VR8 — every gate appears in ≥1 coverage entry.
//!
//! Coverage entries name areas by a free-text `area` field. A gate is treated as
//! "appearing" when its `name` matches the `area` of some audited/skipped/blocked
//! entry; a gate with no such entry is a violation. This reading adds no field to
//! the coverage model.

use serde_json::Value;

use crate::validation::rules::gates;
use crate::validation::{Violation, Vr};

/// Collect the set of `area` strings across audited/skipped/blocked coverage.
fn covered_areas(report: &Value) -> Vec<String> {
    let coverage = match report.get("coverage") {
        Some(c) => c,
        None => return Vec::new(),
    };
    let mut areas = Vec::new();
    for key in ["audited_areas", "skipped_areas", "blocked_areas"] {
        if let Some(entries) = coverage.get(key).and_then(Value::as_array) {
            for entry in entries {
                if let Some(area) = entry.get("area").and_then(Value::as_str) {
                    areas.push(area.to_string());
                }
            }
        }
    }
    areas
}

pub(crate) fn check(report: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    let areas = covered_areas(report);
    for (i, gate) in gates(report) {
        let name = match gate.get("name").and_then(Value::as_str) {
            Some(n) => n,
            None => continue,
        };
        if !areas.iter().any(|a| a == name) {
            out.push(Violation::new(
                Vr::Vr8,
                format!("gates[{i}]"),
                format!("gate `{name}` does not appear in any coverage entry"),
            ));
        }
    }
    out
}
