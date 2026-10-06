//! Confidence rubric (§8; CONF-1/CONF-2; E1/E5).
//!
//! [`recompute_confidence_outcome`] / [`recorded_contradicts`] are the pure
//! rubric and its VR5 compare step; [`ConfidenceRubric`] wires them into a
//! [`RecomputeHook`](crate::validation::RecomputeHook). No rubric parameters are
//! introduced here (those are POLICY), and the `run_vrs` boundary is preserved:
//! VR5 runs only through the hook.

mod rubric;

pub use rubric::{recompute_confidence_outcome, recorded_contradicts, ConfidenceOutcome};

use serde_json::Value;

use crate::finding::Finding;
use crate::validation::{RecomputeHook, Violation, Vr};

/// VR5 recompute-and-compare over a canonical `AuditReport` value.
///
/// For each finding it recomputes the confidence outcome and emits a
/// [`Vr::Vr5`] [`Violation`] when the recorded `confidence` contradicts it (see
/// [`recorded_contradicts`]). `recompute_verdict` keeps the default no-op.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfidenceRubric;

impl RecomputeHook for ConfidenceRubric {
    fn recompute_confidence(&self, report: &Value) -> Vec<Violation> {
        let mut violations = Vec::new();

        let findings = match report.get("findings").and_then(Value::as_array) {
            Some(f) => f,
            None => return violations,
        };

        for (i, finding_value) in findings.iter().enumerate() {
            // A finding that does not deserialize is a schema concern, not a VR5
            // one; skip it rather than inventing a confidence verdict for it.
            let finding: Finding = match serde_json::from_value(finding_value.clone()) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let outcome = recompute_confidence_outcome(&finding);
            if recorded_contradicts(finding.confidence, outcome) {
                violations.push(Violation::new(
                    Vr::Vr5,
                    format!("findings[{i}]"),
                    format!(
                        "recorded confidence {:?} contradicts recomputed outcome {:?} \
                         (deterministic VR5: CONF-2 publish gate / HIGH evidence bar)",
                        finding.confidence, outcome
                    ),
                ));
            }
        }

        violations
    }
}
