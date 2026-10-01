//! VR1 — support-mapping sufficiency.
//!
//! From `schema-formalization.md`: every finding `support_mappings` nonempty;
//! any PASS/VERIFIED has ≥1 SUFFICIENT mapping. The nonempty part (`minItems:1`)
//! is structural (JSON Schema); this module enforces the semantic part:
//!
//! - Every published FINDING (S7, maturity FINDING/RISK) carries ≥1 SUFFICIENT
//!   SupportMapping (CONF-2: LOW ≠ INSUFFICIENT; a published finding is backed by
//!   sufficient evidence).
//! - Every gate in state PASS carries ≥1 SUFFICIENT SupportMapping.
//! - Every finding whose EpistemicState conclusion is VERIFIED carries ≥1
//!   SUFFICIENT SupportMapping.
//!
//! This is a mechanical reading of VR1/CONF-2; it adds no new semantics.

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
