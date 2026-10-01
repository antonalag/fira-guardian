//! One module per non-structural VR. Each exposes `check(...)` returning any
//! [`Violation`](super::Violation)s found. All functions are pure over the
//! parsed report value (plus `ValidationContext` where a rule needs context).

pub(super) mod vr01_sufficiency;
pub(super) mod vr02_failure_anchor;
pub(super) mod vr08_gate_coverage;
pub(super) mod vr09_language;
pub(super) mod vr11_verified_facet;
pub(super) mod vr12_command_fk;
pub(super) mod vr13_write_path;

use serde_json::Value;

/// Iterate a report's `findings` array with their indices, if present.
pub(super) fn findings(report: &Value) -> impl Iterator<Item = (usize, &Value)> {
    report
        .get("findings")
        .and_then(Value::as_array)
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .enumerate()
}

/// Iterate a report's `gates` array with their indices, if present.
pub(super) fn gates(report: &Value) -> impl Iterator<Item = (usize, &Value)> {
    report
        .get("gates")
        .and_then(Value::as_array)
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .enumerate()
}

/// Read `support_mappings` of an object (finding or gate) as a slice.
pub(super) fn support_mappings(obj: &Value) -> &[Value] {
    obj.get("support_mappings")
        .and_then(Value::as_array)
        .map(|a| a.as_slice())
        .unwrap_or(&[])
}

/// True if any mapping in the slice has `sufficiency == "SUFFICIENT"`.
pub(super) fn has_sufficient(mappings: &[Value]) -> bool {
    mappings.iter().any(|m| {
        m.get("sufficiency").and_then(Value::as_str) == Some("SUFFICIENT")
    })
}
