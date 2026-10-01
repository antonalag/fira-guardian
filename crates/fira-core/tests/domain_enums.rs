//! AC-5: enums are closed — out-of-enum values fail to deserialize into the
//! typed model, and the `const: true` markers reject `false`.

mod common;

use common::load_json;
use serde_json::json;

use fira_core::execution::VerificationMechanism;
use fira_core::finding::Finding;
use fira_core::gate::Gate;

/// The Task 2 `vr10-blocker-bad-source.json` fixture is a single S7 Finding whose
/// `requirement_source` is out of enum (`"made_up_source"`). It must fail to
/// deserialize into `Finding`.
#[test]
fn out_of_enum_requirement_source_rejected() {
    let bad = load_json("schemas/examples/invalid/vr10-blocker-bad-source.json");
    let result: Result<Finding, _> = serde_json::from_value(bad);
    assert!(
        result.is_err(),
        "an out-of-enum requirement_source must not deserialize into Finding"
    );
}

/// A crafted Gate with an out-of-enum `state` must fail to deserialize.
#[test]
fn out_of_enum_gate_state_rejected() {
    let bad = json!({
        "name": "Tests",
        "requirement_level": "required",
        "state": "DEFINITELY_PASS",
        "rationale": "r",
        "support_mappings": [],
        "supporting_findings": []
    });
    let result: Result<Gate, _> = serde_json::from_value(bad);
    assert!(result.is_err(), "an out-of-enum gate state must not deserialize");
}

/// `deny_unknown_fields` rejects an unexpected extra field (the Rust analogue of
/// the schema's `additionalProperties:false`).
#[test]
fn unknown_field_rejected() {
    let bad = json!({
        "name": "Tests",
        "requirement_level": "required",
        "state": "PASS",
        "rationale": "r",
        "support_mappings": [],
        "supporting_findings": [],
        "surprise_field": true
    });
    let result: Result<Gate, _> = serde_json::from_value(bad);
    assert!(result.is_err(), "an unknown field must not deserialize (deny_unknown_fields)");
}

/// The `declared_by_project` const-true marker (S12/EXEC-1) rejects `false`.
#[test]
fn declared_by_project_false_rejected() {
    let bad = json!({
        "id": "cmd-x",
        "source": "MAKE",
        "source_locator": "Makefile:1",
        "command": "make test",
        "declared_by_project": false
    });
    let result: Result<VerificationMechanism, _> = serde_json::from_value(bad);
    assert!(
        result.is_err(),
        "declared_by_project=false must not deserialize (schema const: true / EXEC-1)"
    );
}

/// The FIRA-produced human decision rejects a forbidden state (ACCEPTED) — VR6
/// reflected at the type level via the restricted enum.
#[test]
fn fira_human_decision_rejects_accepted() {
    let bad = json!({ "state": "ACCEPTED" });
    let result: Result<fira_core::assessment::HumanDecisionFira, _> =
        serde_json::from_value(bad);
    assert!(
        result.is_err(),
        "HumanDecisionFira must reject ACCEPTED (VR6 restricted enum)"
    );
}
