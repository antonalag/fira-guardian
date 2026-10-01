//! VR11 — VERIFIED requires an execution-based facet. *(correction 1)*
//!
//! From `schema-formalization.md` / frozen contract §3 E1: any
//! `EpistemicState.conclusion=VERIFIED` references ≥1 execution-based facet
//! (OBSERVED, or a TESTED whose test was actually executed with PASSED). Static-
//! only VERIFIED is rejected (no path to VERIFIED from SPECIFIED / IMPLEMENTED /
//! TESTED-not-executed).
//!
//! ## Mechanical check in Task 2
//!
//! The facet set is the mechanically available signal. OBSERVED denotes
//! execution + observation, so it is unambiguously execution-based. TESTED is
//! only execution-based when the test was actually executed and PASSED — a fact
//! that lives in an `ExecutionResult` link, not in the facet itself; that
//! linking is deferred to later tasks. Task 2 therefore enforces the
//! deterministic, non-static portion: an EpistemicState with
//! `conclusion=VERIFIED` must include the OBSERVED facet. This rejects the
//! static-only VERIFIED case (correction 1) without inventing the TESTED-
//! execution linkage that a later task will supply.

use serde_json::Value;

use crate::validation::rules::findings;
use crate::validation::{Violation, Vr};

/// Facets that are unambiguously execution-based from the facet set alone.
const EXECUTION_BASED_FACETS: [&str; 1] = ["OBSERVED"];

fn has_execution_based_facet(epistemic_state: &Value) -> bool {
    epistemic_state
        .get("facets")
        .and_then(Value::as_array)
        .map(|facets| {
            facets
                .iter()
                .filter_map(Value::as_str)
                .any(|f| EXECUTION_BASED_FACETS.contains(&f))
        })
        .unwrap_or(false)
}

pub(crate) fn check(report: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    for (i, finding) in findings(report) {
        let epistemic_state = match finding.get("epistemic_state") {
            Some(e) => e,
            None => continue,
        };
        let verified =
            epistemic_state.get("conclusion").and_then(Value::as_str) == Some("VERIFIED");
        if verified && !has_execution_based_facet(epistemic_state) {
            out.push(Violation::new(
                Vr::Vr11,
                format!("findings[{i}].epistemic_state"),
                "conclusion=VERIFIED without an execution-based facet (OBSERVED); static-only VERIFIED is rejected",
            ));
        }
    }
    out
}
