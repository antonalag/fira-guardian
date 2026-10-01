//! AC-2, AC-3, AC-4, AC-5.
//!
//! - AC-2: every JSON Schema is valid against its declared dialect (Draft
//!   2020-12 meta-schema) — proven by compiling each schema (compilation fails
//!   on a malformed schema).
//! - AC-3: every valid example passes its schema.
//! - AC-4: every invalid example fails at its expected site (schema for
//!   structural VRs; asserted here for the structural set).
//! - AC-5: structural VRs (VR3, VR4, VR6) are enforced in the JSON Schema — each
//!   proven by a fixture that only that rule rejects.

mod common;

use serde_json::{json, Value};

use common::{compile, compile_schema_file, load_json, SCHEMA_FILES};

/// AC-2: each of the 12 schemas compiles (meta-schema valid) with cross-refs
/// resolved by the bundled resolver.
#[test]
fn all_schemas_compile() {
    for file in SCHEMA_FILES {
        let _ = compile_schema_file(file);
    }
}

/// Wrap a `$defs`-only schema so an instance can be validated against one of its
/// definitions by `$ref`.
fn defs_ref(schema_id: &str, def: &str) -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$ref": format!("{schema_id}#/$defs/{def}")
    })
}

/// A valid-example case: (example file under `schemas/examples/valid`, schema
/// file, optional `(schema $id, $defs entry)` for `$defs`-only schemas).
type ValidCase = (&'static str, &'static str, Option<(&'static str, &'static str)>);

/// AC-3: valid examples pass. Table maps example file -> the schema (or a
/// $defs entry) it must validate against.
#[test]
fn valid_examples_pass() {
    let cases: &[ValidCase] = &[
        ("s01-audit-request.json", "s01-audit-request.schema.json", None),
        ("s02-audit-profile.json", "s02-audit-profile.schema.json", None),
        ("s03-epistemic-state.json", "s03-epistemic-state.schema.json", None),
        (
            "s04-evidence-ref.json",
            "s04-evidence-support-mapping.schema.json",
            Some(("urn:fira:schema:1.0-frozen+corr1-4:s04-evidence-support-mapping", "EvidenceRef")),
        ),
        (
            "s04-support-mapping.json",
            "s04-evidence-support-mapping.schema.json",
            Some(("urn:fira:schema:1.0-frozen+corr1-4:s04-evidence-support-mapping", "SupportMapping")),
        ),
        ("s05-execution-result.json", "s05-execution-result.schema.json", None),
        (
            "s06-declared-claim.json",
            "s06-requirement-family.schema.json",
            Some(("urn:fira:schema:1.0-frozen+corr1-4:s06-requirement-family", "DeclaredClaim")),
        ),
        ("s07-finding.json", "s07-finding.schema.json", None),
        ("s08-gate.json", "s08-gate.schema.json", None),
        ("s09-coverage-statement.json", "s09-coverage-statement.schema.json", None),
        (
            "s10-technical-assessment.json",
            "s10-assessment-decision.schema.json",
            Some(("urn:fira:schema:1.0-frozen+corr1-4:s10-assessment-decision", "TechnicalAssessment")),
        ),
        (
            "s10-human-decision-fira.json",
            "s10-assessment-decision.schema.json",
            Some(("urn:fira:schema:1.0-frozen+corr1-4:s10-assessment-decision", "HumanDecisionFira")),
        ),
        ("s11-audit-report.json", "s11-audit-report.schema.json", None),
        ("s12-verification-mechanism.json", "s12-verification-mechanism.schema.json", None),
    ];

    for (example, schema_file, defs) in cases {
        let instance = load_json(&format!("schemas/examples/valid/{example}"));
        let validator = match defs {
            None => compile_schema_file(schema_file),
            Some((id, def)) => compile(&defs_ref(id, def)),
        };
        let errors: Vec<String> = match validator.validate(&instance) {
            Ok(()) => Vec::new(),
            Err(errs) => errs.map(|e| e.to_string()).collect(),
        };
        assert!(
            errors.is_empty(),
            "valid example {example} failed its schema: {errors:?}"
        );
    }
}

/// AC-4 + AC-5: structural VR fixtures fail purely on the JSON Schema, and each
/// is otherwise valid so only that rule rejects it.
#[test]
fn structural_vr_fixtures_fail_at_schema() {
    // VR3: EvidenceRef with type=COMMAND but no captured_output_ref.
    let vr3 = load_json("schemas/examples/invalid/vr03-command-missing-output.json");
    let ev = compile(&defs_ref(
        "urn:fira:schema:1.0-frozen+corr1-4:s04-evidence-support-mapping",
        "EvidenceRef",
    ));
    assert!(
        !ev.is_valid(&vr3),
        "VR3 fixture must be rejected by the EvidenceRef schema"
    );

    // VR4: Gate state=N/A with requirement_level != not_applicable.
    let vr4 = load_json("schemas/examples/invalid/vr04-na-mismatch.json");
    let gate = compile_schema_file("s08-gate.schema.json");
    assert!(!gate.is_valid(&vr4), "VR4 fixture must be rejected by the Gate schema");

    // VR6: FIRA-produced HumanDecision with a forbidden state (ACCEPTED).
    let vr6 = load_json("schemas/examples/invalid/vr06-fira-accepted.json");
    let hd = compile(&defs_ref(
        "urn:fira:schema:1.0-frozen+corr1-4:s10-assessment-decision",
        "HumanDecisionFira",
    ));
    assert!(
        !hd.is_valid(&vr6),
        "VR6 fixture must be rejected by the HumanDecisionFira schema"
    );

    // VR10 (structural-only in Task 2): out-of-enum requirement_source on a
    // BLOCKER finding is rejected by the Finding schema.
    let vr10 = load_json("schemas/examples/invalid/vr10-blocker-bad-source.json");
    let finding = compile_schema_file("s07-finding.schema.json");
    assert!(
        !finding.is_valid(&vr10),
        "VR10 fixture must be rejected by the Finding schema (enum guard)"
    );
}

/// AC-5 sharpness: each structural fixture is *otherwise* schema-valid — the
/// corrected version passes, proving the fixture fails solely on the target rule.
#[test]
fn structural_vr_fixtures_are_otherwise_valid() {
    let ev = compile(&defs_ref(
        "urn:fira:schema:1.0-frozen+corr1-4:s04-evidence-support-mapping",
        "EvidenceRef",
    ));
    let mut vr3 = load_json("schemas/examples/invalid/vr03-command-missing-output.json");
    vr3["captured_output_ref"] = json!("cmd-tests");
    assert!(ev.is_valid(&vr3), "VR3 fixture becomes valid once captured_output_ref is added");

    let gate = compile_schema_file("s08-gate.schema.json");
    let mut vr4 = load_json("schemas/examples/invalid/vr04-na-mismatch.json");
    vr4["requirement_level"] = json!("not_applicable");
    assert!(gate.is_valid(&vr4), "VR4 fixture becomes valid once requirement_level=not_applicable");

    let hd = compile(&defs_ref(
        "urn:fira:schema:1.0-frozen+corr1-4:s10-assessment-decision",
        "HumanDecisionFira",
    ));
    let mut vr6 = load_json("schemas/examples/invalid/vr06-fira-accepted.json");
    vr6["state"] = json!("PENDING");
    assert!(hd.is_valid(&vr6), "VR6 fixture becomes valid once state is PENDING");
}

/// AC-4 (semantic fixtures are schema-valid): the validator-targeted invalid
/// fixtures are full reports that PASS the S11 schema — their rejection is the
/// validator's job (asserted in vr_validator.rs), not the schema's.
#[test]
fn semantic_vr_fixtures_are_schema_valid() {
    let report = compile_schema_file("s11-audit-report.schema.json");
    for fixture in [
        "vr01-no-sufficient-mapping.json",
        "vr02-recovery-missing-anchor.json",
        "vr05-surface-insufficient-finding.json",
        "vr08-gate-without-coverage.json",
        "vr09-safe-language.json",
        "vr11-static-only-verified.json",
        "vr12-synthesized-command.json",
        "vr13-write-under-project-root.json",
    ] {
        let instance = load_json(&format!("schemas/examples/invalid/{fixture}"));
        let errors: Vec<String> = match report.validate(&instance) {
            Ok(()) => Vec::new(),
            Err(errs) => errs.map(|e| e.to_string()).collect(),
        };
        assert!(
            errors.is_empty(),
            "semantic fixture {fixture} should be schema-valid but failed: {errors:?}"
        );
    }
}

/// AC-3 (golden): the golden composite report passes the S11 schema.
#[test]
fn golden_report_passes_schema() {
    let report = compile_schema_file("s11-audit-report.schema.json");
    let golden = load_json("schemas/examples/valid/s11-audit-report.json");
    assert!(report.is_valid(&golden), "golden AuditReport must pass the S11 schema");
}
