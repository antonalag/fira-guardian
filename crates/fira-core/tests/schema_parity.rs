//! AC-1, AC-9, AC-10.
//!
//! - AC-1: twelve schema files exist, each with `$schema`, `$id`, `title`, and a
//!   field set matching `schema-formalization.md` (no extra/missing fields).
//! - AC-9: no schema or VR is added, removed, renumbered, or restated — the
//!   produced artifacts cover exactly S1..S12 and VR1..VR13 (no more, no fewer),
//!   cross-checked by grepping the schema doc.
//! - AC-10: the `$id` contract-version segment equals `fira_core::CONTRACT_VERSION`.

mod common;

use std::collections::BTreeSet;

use serde_json::Value;

use common::{load_json, schemas_dir, workspace_root, SCHEMA_FILES};

/// AC-1: each schema has $schema, $id, title present.
#[test]
fn schemas_have_required_metadata() {
    for file in SCHEMA_FILES {
        let schema = load_json(&format!("schemas/{file}"));
        for key in ["$schema", "$id", "title"] {
            assert!(
                schema.get(key).and_then(Value::as_str).is_some(),
                "{file} missing string `{key}`"
            );
        }
        // Draft 2020-12 dialect fixed (design section 2).
        assert_eq!(
            schema["$schema"].as_str().unwrap(),
            "https://json-schema.org/draft/2020-12/schema",
            "{file} must declare the Draft 2020-12 dialect"
        );
    }
}

/// AC-10: every schema `$id` embeds the frozen contract version segment, equal to
/// `fira_core::CONTRACT_VERSION`.
#[test]
fn schema_ids_carry_contract_version() {
    let version = fira_core::CONTRACT_VERSION;
    let expected_prefix = format!("urn:fira:schema:{version}:");
    for file in SCHEMA_FILES {
        let schema = load_json(&format!("schemas/{file}"));
        let id = schema["$id"].as_str().unwrap();
        assert!(
            id.starts_with(&expected_prefix),
            "{file} $id `{id}` must embed contract version segment `{version}`"
        );
    }
    // The shared defs file too.
    let common = load_json("schemas/_defs/common.schema.json");
    assert!(common["$id"]
        .as_str()
        .unwrap()
        .starts_with(&expected_prefix));
}

/// Expected top-level required+optional property sets per schema, transcribed
/// from `schema-formalization.md` §S1..S12. For `$defs`-only schemas (S4, S6,
/// S10) the fields are checked per-definition.
///
/// This manifest is the AC-1 field-parity oracle: the test asserts the schema's
/// declared property set equals exactly this set (no extra, no missing).
fn expected_properties() -> Vec<(&'static str, Option<&'static str>, BTreeSet<&'static str>)> {
    fn set(items: &[&'static str]) -> BTreeSet<&'static str> {
        items.iter().copied().collect()
    }
    vec![
        (
            "s01-audit-request.schema.json",
            None,
            set(&[
                "audit_id",
                "project_root",
                "release_target",
                "execution_boundary",
                "declared_claims",
                "audit_profile",
                "prior_audit_report_ref",
                "requested_depth",
            ]),
        ),
        (
            "s02-audit-profile.schema.json",
            None,
            set(&[
                "profile_id",
                "version",
                "description",
                "gates",
                "focus_areas",
                "typical_failure_modes",
                "minimum_evidence_expectations",
            ]),
        ),
        (
            "s03-epistemic-state.schema.json",
            None,
            set(&["facets", "conclusion"]),
        ),
        (
            "s04-evidence-support-mapping.schema.json",
            Some("EvidenceRef"),
            set(&["ref_id", "type", "locator", "valid", "captured_output_ref"]),
        ),
        (
            "s04-evidence-support-mapping.schema.json",
            Some("SupportMapping"),
            set(&[
                "claim",
                "evidence_refs",
                "relevant_span",
                "sufficiency",
                "unverified_remainder",
            ]),
        ),
        (
            "s05-execution-result.schema.json",
            None,
            set(&[
                "command_id",
                "command",
                "outcome",
                "classification",
                "blocking_condition",
                "captured_output",
                "exercised_behaviors",
            ]),
        ),
        (
            "s06-requirement-family.schema.json",
            Some("DeclaredClaim"),
            set(&["id", "statement", "source_ref", "security_relevant"]),
        ),
        (
            "s06-requirement-family.schema.json",
            Some("InferredInvariant"),
            set(&["id", "statement", "derivation_evidence", "inferred", "code_internal_only"]),
        ),
        (
            "s06-requirement-family.schema.json",
            Some("ReleaseRequirement"),
            set(&["id", "statement", "from_profile", "gate"]),
        ),
        (
            "s06-requirement-family.schema.json",
            Some("SecurityProperty"),
            set(&["id", "statement", "source", "underlying_ref"]),
        ),
        (
            "s07-finding.schema.json",
            None,
            set(&[
                "id",
                "maturity",
                "title",
                "severity",
                "confidence",
                "confidence_derivation",
                "category",
                "requirement_source",
                "security_property",
                "gates_affected",
                "epistemic_state",
                "support_mappings",
                "failure_scenario",
                "impact",
                "release_impact",
                "recommended_remediation",
                "verification_criteria",
                "lifecycle_status",
                "regression",
            ]),
        ),
        (
            "s08-gate.schema.json",
            None,
            set(&[
                "name",
                "requirement_level",
                "state",
                "rationale",
                "cause",
                "support_mappings",
                "supporting_findings",
            ]),
        ),
        (
            "s09-coverage-statement.schema.json",
            None,
            set(&[
                "audited_areas",
                "skipped_areas",
                "blocked_areas",
                "executed_mechanisms",
                "known_limitations",
                "assumptions",
                "unanchored_hypotheses",
            ]),
        ),
        (
            "s10-assessment-decision.schema.json",
            Some("TechnicalAssessment"),
            set(&[
                "result",
                "blocking_findings",
                "blocking_gates",
                "risk_findings",
                "risk_gates",
                "computed_by_rule",
            ]),
        ),
        (
            "s10-assessment-decision.schema.json",
            Some("HumanDecision"),
            set(&["state"]),
        ),
        (
            "s10-assessment-decision.schema.json",
            Some("HumanDecisionFira"),
            set(&["state"]),
        ),
        (
            "s11-audit-report.schema.json",
            None,
            set(&[
                "schema_version",
                "audit_id",
                "created_at",
                "request",
                "applied_profile",
                "system_model_summary",
                "verification_mechanisms",
                "divergence_records",
                "findings",
                "gates",
                "coverage",
                "technical_assessment",
                "human_decision",
                "prior_report_ref",
                "determinism_inputs_hash",
            ]),
        ),
        (
            "s12-verification-mechanism.schema.json",
            None,
            set(&["id", "source", "source_locator", "command", "declared_by_project"]),
        ),
    ]
}

fn property_names(schema: &Value, def: Option<&str>) -> BTreeSet<String> {
    let props = match def {
        None => schema.get("properties"),
        Some(d) => schema
            .get("$defs")
            .and_then(|defs| defs.get(d))
            .and_then(|s| s.get("properties")),
    };
    props
        .and_then(Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

/// AC-1: each schema's declared property set matches the doc field set exactly.
#[test]
fn schema_field_parity() {
    for (file, def, expected) in expected_properties() {
        let schema = load_json(&format!("schemas/{file}"));
        let actual = property_names(&schema, def);
        let expected: BTreeSet<String> = expected.into_iter().map(String::from).collect();
        assert_eq!(
            actual,
            expected,
            "field parity mismatch in {file}{}",
            def.map(|d| format!(" (${d})")).unwrap_or_default()
        );
    }
}

/// AC-1: every object schema forbids extra fields (`additionalProperties:false`),
/// guaranteeing exact parity (no silent extra fields).
#[test]
fn objects_forbid_additional_properties() {
    fn walk(value: &Value, path: &str, violations: &mut Vec<String>) {
        if let Value::Object(map) = value {
            // A concrete object *definition* declares `"type": "object"`. The
            // if/then/else and not fragments used to encode structural VRs are
            // constraint subschemas, not object definitions, so they legitimately
            // omit additionalProperties; only real object definitions must set
            // additionalProperties:false for exact field parity (AC-1).
            let is_object_schema = map.get("type").and_then(Value::as_str) == Some("object");
            if is_object_schema
                && map.get("additionalProperties") != Some(&Value::Bool(false))
            {
                violations.push(path.to_string());
            }
            for (k, v) in map {
                walk(v, &format!("{path}/{k}"), violations);
            }
        } else if let Value::Array(arr) = value {
            for (i, v) in arr.iter().enumerate() {
                walk(v, &format!("{path}/{i}"), violations);
            }
        }
    }
    for file in SCHEMA_FILES {
        let schema = load_json(&format!("schemas/{file}"));
        let mut violations = Vec::new();
        walk(&schema, file, &mut violations);
        assert!(
            violations.is_empty(),
            "object schemas without additionalProperties:false in {file}: {violations:?}"
        );
    }
}

/// AC-9: exact S1..S12 coverage. The schema doc defines S1..S12; exactly twelve
/// schema files exist, named s01..s12, and no s13+ exists.
#[test]
fn exact_schema_coverage() {
    let doc = std::fs::read_to_string(workspace_root().join("docs/contract/schema-formalization.md"))
        .expect("read schema doc");

    // Which S-numbers the doc defines (headings like "### S1." ... "### S12.").
    let mut doc_snums: BTreeSet<u32> = BTreeSet::new();
    for n in 1..=99u32 {
        if doc.contains(&format!("### S{n}.")) {
            doc_snums.insert(n);
        }
    }
    let expected: BTreeSet<u32> = (1..=12).collect();
    assert_eq!(doc_snums, expected, "schema doc must define exactly S1..S12");

    // Exactly the twelve files exist under schemas/ (plus _defs/common).
    let mut files: Vec<String> = std::fs::read_dir(schemas_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".schema.json"))
        .collect();
    files.sort();
    let mut expected_files: Vec<String> = SCHEMA_FILES.iter().map(|s| s.to_string()).collect();
    expected_files.sort();
    assert_eq!(files, expected_files, "exactly S1..S12 schema files must exist");
}

/// AC-9: exact VR1..VR13 coverage. The schema doc enumerates exactly VR1..VR13.
#[test]
fn exact_vr_coverage() {
    let doc = std::fs::read_to_string(workspace_root().join("docs/contract/schema-formalization.md"))
        .expect("read schema doc");
    let mut doc_vrs: BTreeSet<u32> = BTreeSet::new();
    for n in 1..=99u32 {
        if doc.contains(&format!("**VR{n}**")) {
            doc_vrs.insert(n);
        }
    }
    let expected: BTreeSet<u32> = (1..=13).collect();
    assert_eq!(doc_vrs, expected, "schema doc must enumerate exactly VR1..VR13");
}
