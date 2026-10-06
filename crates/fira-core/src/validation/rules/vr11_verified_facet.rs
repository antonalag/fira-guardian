//! VR11 — `conclusion=VERIFIED` requires an execution-based facet.
//!
//! Only OBSERVED is decidable from the facet set alone, so this check requires a
//! VERIFIED epistemic state to include OBSERVED. "Executed-PASSED TESTED" is also
//! execution-based per the contract, but that fact lives in an `ExecutionResult`
//! link not present in the facet set, so it is not mechanized here. Requiring
//! OBSERVED still rejects the static-only VERIFIED case.

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
