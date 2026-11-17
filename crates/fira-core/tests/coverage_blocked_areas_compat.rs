//! failure-classification-slice (AC-16): S9/S11 and VR8 accept a non-empty
//! `coverage.blocked_areas`.
//!
//! The bundled fixtures only ever carry `blocked_areas: []`, so this pins, with
//! the real JSON Schemas and the real validator, that the representation the
//! slice will emit for an INFRA impediment (a gate-named `area`, a
//! `blocking_condition`, and an `execution_result_ref` naming a declared
//! mechanism) is schema-valid and satisfies VR8 on its own. Read-only over the
//! golden report; every mutation is in memory.

mod common;

use common::{compile_schema_file, load_json};
use serde_json::{json, Value};

use fira_core::validation::{run_vrs, StaticValidationContext, Vr};

fn schema_errors(schema_file: &str, instance: &Value) -> Vec<String> {
    let validator = compile_schema_file(schema_file);
    let errors: Vec<String> = match validator.validate(instance) {
        Ok(()) => Vec::new(),
        Err(errs) => errs.map(|e| e.to_string()).collect(),
    };
    errors
}

/// The golden report with its only gate (`Tests`) recorded as blocked by an
/// infrastructure impediment: UNKNOWN / INFRA, an INSUFFICIENT mapping, and a
/// `blocked_areas` entry instead of the `audited_areas` one.
fn report_with_blocked_tests_gate() -> Value {
    let mut report = load_json("schemas/examples/valid/s11-audit-report.json");
    report["gates"][0]["state"] = json!("UNKNOWN");
    report["gates"][0]["cause"] = json!("INFRA");
    report["gates"][0]["rationale"] = json!(
        "the project's declared test mechanism did not produce a conclusive result because \
         of an infrastructure impediment (INFRA_LEVEL) identified by the execution layer; not \
         sufficiently audited"
    );
    report["gates"][0]["support_mappings"] = json!([{
        "claim": "declared mechanism cmd-tests executed with outcome Error",
        "evidence_refs": ["cmd-tests"],
        "relevant_span": "Makefile:12",
        "sufficiency": "INSUFFICIENT",
        "unverified_remainder": "mechanism could not be evaluated because of an infrastructure impediment (Error, INFRA_LEVEL); it provides no evidence about the application"
    }]);
    report["coverage"]["audited_areas"] = json!([]);
    report["coverage"]["blocked_areas"] = json!([{
        "area": "Tests",
        "blocking_condition": "failed to start `make`: No such file or directory (os error 2)",
        "execution_result_ref": "cmd-tests"
    }]);
    report["coverage"]["executed_mechanisms"] = json!([
        { "command_id": "cmd-tests", "outcome": "ERROR" }
    ]);
    report
}

/// AC-16: a non-empty `blocked_areas` is valid against S11 (and S9 for the
/// coverage statement alone), and VR8 counts a gate that appears only there.
#[test]
fn blocked_areas_nonempty_validates_against_s11_and_vr8() {
    // Precondition: the unmodified golden report is clean.
    let golden = load_json("schemas/examples/valid/s11-audit-report.json");
    assert!(schema_errors("s11-audit-report.schema.json", &golden).is_empty());
    let golden_ctx = StaticValidationContext::from_report(&golden);
    assert!(run_vrs(&golden, &golden_ctx).is_empty());

    let report = report_with_blocked_tests_gate();

    let errors = schema_errors("s11-audit-report.schema.json", &report);
    assert!(
        errors.is_empty(),
        "S11 must accept blocked_areas: {errors:?}"
    );
    let errors = schema_errors("s09-coverage-statement.schema.json", &report["coverage"]);
    assert!(
        errors.is_empty(),
        "S9 must accept blocked_areas: {errors:?}"
    );

    let ctx = StaticValidationContext::from_report(&report);
    let violations = run_vrs(&report, &ctx);
    assert!(
        violations.is_empty(),
        "a gate covered only by blocked_areas must pass every run_vrs check (VR8 included); \
         got {violations:?}"
    );
}

/// Negative control: VR8 genuinely reads `blocked_areas` — removing the entry
/// leaves the gate in no coverage list and VR8 fires.
#[test]
fn removing_the_blocked_entry_triggers_vr8() {
    let mut report = report_with_blocked_tests_gate();
    report["coverage"]["blocked_areas"] = json!([]);
    let ctx = StaticValidationContext::from_report(&report);
    let violations = run_vrs(&report, &ctx);
    assert!(
        violations.iter().any(|v| v.vr == Vr::Vr8),
        "a gate in no coverage list must violate VR8; got {violations:?}"
    );
}

/// The S9 item shape is enforced: an entry without `execution_result_ref`, or
/// with an empty one (IdRef has minLength 1), is rejected by the schema.
#[test]
fn malformed_blocked_entries_are_rejected_by_schema() {
    let mut missing_ref = report_with_blocked_tests_gate();
    missing_ref["coverage"]["blocked_areas"] = json!([{
        "area": "Tests",
        "blocking_condition": "infrastructure impediment"
    }]);
    assert!(
        !schema_errors("s11-audit-report.schema.json", &missing_ref).is_empty(),
        "execution_result_ref is required"
    );

    let mut empty_ref = report_with_blocked_tests_gate();
    empty_ref["coverage"]["blocked_areas"][0]["execution_result_ref"] = json!("");
    assert!(
        !schema_errors("s11-audit-report.schema.json", &empty_ref).is_empty(),
        "execution_result_ref must be a non-empty IdRef"
    );
}
