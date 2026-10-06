//! Deterministic verdict engine (frozen contract §11; C10) — Task 7.
//!
//! Public surface:
//! - [`RecomputedAssessment`], [`ExpectationViolations`], and
//!   [`compute_assessment`] — the pure, deterministic §11 Steps 1–4 over the
//!   typed [`Gate`](crate::gate::Gate) / [`Finding`](crate::finding::Finding)
//!   model.
//! - [`VerdictEngine`] — a [`RecomputeHook`](crate::validation::RecomputeHook)
//!   implementor performing VR7 recompute-and-compare over a report value.
//!
//! Scope (approved Option A, Task 7):
//! - Pure, deterministic, in CORE; never reads the recorded assessment (C10).
//! - Consumes recorded `Gate.state`; does not re-derive gate states (§7/Step0).
//! - `minimum_evidence_expectations` are POLICY (Task 8); supplied via
//!   [`ExpectationViolations`] (default empty ⇒ required PARTIAL = RISK).
//! - VR7 compares the result **and** the blocking/risk sets, order-insensitively.
//! - `determinism_inputs_hash` is not computed/verified (no fixed algorithm).
//! - Keeps the Task 2 `RecomputeHook` boundary: `run_vrs` is **not** modified.
//! - No host FS/process/network.

mod engine;

pub use engine::{compute_assessment, ExpectationViolations, RecomputedAssessment};

use serde_json::Value;

use crate::finding::Finding;
use crate::gate::Gate;
use crate::validation::{RecomputeHook, Violation, Vr};

/// VR7 recompute-and-compare over a canonical `AuditReport` value.
///
/// Zero-sized and pure. Implements [`RecomputeHook::recompute_verdict`]: it
/// recomputes the assessment from the report's gates + findings (never reading
/// the recorded `technical_assessment`, C10) and emits a [`Vr::Vr7`]
/// [`Violation`] when the recorded `technical_assessment.result` or any of its
/// blocking/risk sets deviate from the recompute. `recompute_confidence` keeps
/// the default no-op — the confidence rubric (VR5) is Task 6.
#[derive(Debug, Clone, Copy, Default)]
pub struct VerdictEngine;

/// Order-insensitive equality of two id lists (compare as sets: same length and
/// mutual containment). Used so the blocking/risk-set comparison does not depend
/// on ordering.
fn set_eq<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.contains(x)) && b.iter().all(|y| a.contains(y))
}

impl RecomputeHook for VerdictEngine {
    fn recompute_verdict(&self, report: &Value) -> Vec<Violation> {
        let mut violations = Vec::new();

        // Deserialize the typed inputs. If gates/findings or the recorded
        // assessment are not present/parseable, that is a schema-layer concern
        // (Task 2), not VR7; do not fabricate a verdict in that case.
        let gates: Vec<Gate> = match report.get("gates").cloned().map(serde_json::from_value) {
            Some(Ok(g)) => g,
            _ => return violations,
        };
        let findings: Vec<Finding> =
            match report.get("findings").cloned().map(serde_json::from_value) {
                Some(Ok(f)) => f,
                _ => return violations,
            };

        let recorded = match report.get("technical_assessment") {
            Some(v) => v,
            None => return violations,
        };

        // Recompute with the default (empty) expectation violations: with no
        // POLICY minimum_evidence_expectations yet, required PARTIAL ⇒ RISK
        // (§11 else; Task 7 §9.1(A)).
        let recomputed = compute_assessment(&gates, &findings, &ExpectationViolations::none());

        // Compare `result`.
        let recorded_result = recorded.get("result").and_then(Value::as_str);
        let recomputed_result = technical_result_wire(&recomputed.result);
        if recorded_result != Some(recomputed_result) {
            violations.push(Violation::new(
                Vr::Vr7,
                "technical_assessment.result",
                format!(
                    "recorded result {:?} differs from recomputed {recomputed_result:?}",
                    recorded_result.unwrap_or("<missing>")
                ),
            ));
        }

        // Compare each blocking/risk set, order-insensitively.
        compare_set(
            &mut violations,
            recorded,
            "blocking_findings",
            &id_strings(&recomputed.blocking_findings),
        );
        compare_set(
            &mut violations,
            recorded,
            "blocking_gates",
            &gatename_strings(&recomputed.blocking_gates),
        );
        compare_set(
            &mut violations,
            recorded,
            "risk_findings",
            &id_strings(&recomputed.risk_findings),
        );
        compare_set(
            &mut violations,
            recorded,
            "risk_gates",
            &gatename_strings(&recomputed.risk_gates),
        );

        violations
    }
}

/// Compare a recorded string array field against the recomputed set.
fn compare_set(
    violations: &mut Vec<Violation>,
    recorded: &Value,
    field: &str,
    recomputed: &[String],
) {
    let recorded_set: Vec<String> = recorded
        .get(field)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();

    if !set_eq(&recorded_set, recomputed) {
        violations.push(Violation::new(
            Vr::Vr7,
            format!("technical_assessment.{field}"),
            format!(
                "recorded {field} {recorded_set:?} differs from recomputed {recomputed:?} (set-insensitive)"
            ),
        ));
    }
}

/// The wire string for a `TechnicalAssessmentResult` (matches the S10 enum).
fn technical_result_wire(result: &crate::model::TechnicalAssessmentResult) -> &'static str {
    use crate::model::TechnicalAssessmentResult as R;
    match result {
        R::Ready => "READY",
        R::ReadyWithRisks => "READY_WITH_RISKS",
        R::NotReady => "NOT_READY",
    }
}

fn id_strings(ids: &[crate::model::FindingId]) -> Vec<String> {
    ids.iter().map(|id| id.0.clone()).collect()
}

fn gatename_strings(gates: &[crate::model::GateName]) -> Vec<String> {
    // Serialize each GateName to its wire string via serde_json (single source of
    // truth for the enum's string form).
    gates
        .iter()
        .map(|g| {
            serde_json::to_value(g)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default()
        })
        .collect()
}
