//! VR1 — support-mapping sufficiency (the semantic part; the nonempty `minItems`
//! part is structural).
//!
//! Requires ≥1 SUFFICIENT SupportMapping for: every published finding (CONF-2:
//! LOW ≠ INSUFFICIENT), every gate in state PASS, and every VERIFIED finding.

use serde_json::Value;

use crate::validation::rules::{findings, gates, has_sufficient, support_mappings};
use crate::validation::{Violation, Vr};

pub(crate) fn check(report: &Value) -> Vec<Violation> {
    let mut out = Vec::new();

    for (i, finding) in findings(report) {
        let mappings = support_mappings(finding);
        let verified = finding
            .get("epistemic_state")
            .and_then(|e| e.get("conclusion"))
            .and_then(Value::as_str)
            == Some("VERIFIED");

        // Every published FINDING/RISK needs a SUFFICIENT mapping (CONF-2); a
        // VERIFIED finding independently needs one (PASS/VERIFIED rule).
        if !has_sufficient(mappings) {
            let reason = if verified {
                "finding with conclusion=VERIFIED has no SUFFICIENT support_mapping"
            } else {
                "published finding has no SUFFICIENT support_mapping (CONF-2: LOW != INSUFFICIENT)"
            };
            out.push(Violation::new(Vr::Vr1, format!("findings[{i}]"), reason));
        }
    }

    for (i, gate) in gates(report) {
        let state = gate.get("state").and_then(Value::as_str);
        if state == Some("PASS") && !has_sufficient(support_mappings(gate)) {
            out.push(Violation::new(
                Vr::Vr1,
                format!("gates[{i}]"),
                "gate in state PASS has no SUFFICIENT support_mapping",
            ));
        }
    }

    out
}
