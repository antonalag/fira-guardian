//! Confidence rubric (frozen contract §8; CONF-1/CONF-2; E1/E5) — Task 6.
//!
//! Public surface:
//! - [`ConfidenceOutcome`] and [`recompute_confidence_outcome`] — the pure,
//!   deterministic rubric over the typed [`Finding`](crate::finding::Finding)
//!   model.
//! - [`recorded_contradicts`] — the deterministic VR5 compare step.
//! - [`ConfidenceRubric`] — a [`RecomputeHook`](crate::validation::RecomputeHook)
//!   implementor that performs VR5 recompute-and-compare over a report value.
//!
//! Scope (approved Option A, Task 6):
//! - Implements only the contract-fixed deterministic portion of §8/VR5.
//! - Does **not** mechanize the MEDIUM/LOW distinction (no structured signal).
//! - Introduces **no** rubric parameters (those are POLICY).
//! - Keeps the Task 2 `RecomputeHook` boundary: `run_vrs` is **not** modified to
//!   call the rubric; VR5 is invoked explicitly via the hook.
//! - Pure CORE computation: no filesystem, process, or network.

mod rubric;

pub use rubric::{recompute_confidence_outcome, recorded_contradicts, ConfidenceOutcome};

use serde_json::Value;

use crate::finding::Finding;
use crate::validation::{RecomputeHook, Violation, Vr};

/// VR5 recompute-and-compare over a canonical `AuditReport` value.
///
/// Zero-sized and pure. Implements [`RecomputeHook::recompute_confidence`]: for
/// each published finding it recomputes the confidence outcome from the evidence
/// model and emits a [`Vr::Vr5`] [`Violation`] when the recorded `confidence`
/// deterministically contradicts the recomputed outcome (see
/// [`recorded_contradicts`]). `recompute_verdict` keeps the default no-op — the
/// verdict engine (VR7) is Task 7.
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
            // Deserialize into the typed model so the rubric is typed and the
            // compare is exact. A finding that does not deserialize is a schema
            // concern (handled by the Task 2 schema layer), not a VR5 concern;
            // skip it here rather than inventing a confidence verdict for it.
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
