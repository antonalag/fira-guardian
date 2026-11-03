//! report-provenance-slice Task 3: `fira-dih/1` hash + `assess_with_inputs_hash`.
//!
//! Inputs are built from constructed `Gate`/`Finding` values (deserialized from
//! inline JSON, the same pattern as `verdict_engine.rs`). Expected values are
//! pinned literals; no test reads the clock or runs an external command.

use serde_json::{json, Value};

use fira_core::finding::Finding;
use fira_core::gate::Gate;
use fira_core::model::{GateName, TechnicalAssessmentResult};
use fira_core::provenance::{
    assess_with_inputs_hash, determinism_inputs_hash, format_utc_timestamp, sha256_hex,
    DETERMINISM_HASH_PREFIX, DETERMINISM_HASH_SCHEME,
};
use fira_core::verdict::{compute_assessment, ExpectationViolations};

// ---- builders --------------------------------------------------------------

fn gate_json(name: &str, level: &str, state: &str) -> Value {
    json!({
        "name": name,
        "requirement_level": level,
        "state": state,
        "rationale": "r",
        "support_mappings": [
            {
                "claim": "c",
                "evidence_refs": ["make-test"],
                "relevant_span": "Makefile:3",
                "sufficiency": "SUFFICIENT"
            }
        ],
        "supporting_findings": []
    })
}

fn gate(name: &str, level: &str, state: &str) -> Gate {
    serde_json::from_value(gate_json(name, level, state)).expect("gate deserializes")
}

fn gate_with(edit: impl FnOnce(&mut Value)) -> Gate {
    let mut v = gate_json("Tests", "required", "PASS");
    edit(&mut v);
    serde_json::from_value(v).expect("gate deserializes")
}

fn finding_json() -> Value {
    json!({
        "id": "RR-001",
        "maturity": "FINDING",
        "title": "t",
        "severity": "LOW",
        "confidence": "HIGH",
        "confidence_derivation": "d",
        "category": "input-validation",
        "requirement_source": "declared_claim",
        "gates_affected": ["Tests"],
        "epistemic_state": { "facets": ["OBSERVED"], "conclusion": "VERIFIED" },
        "support_mappings": [
            { "claim": "c", "evidence_refs": ["E-1"], "relevant_span": "s", "sufficiency": "SUFFICIENT" }
        ],
        "impact": "i",
        "release_impact": "NON_BLOCKING",
        "recommended_remediation": "r",
        "verification_criteria": "v",
        "lifecycle_status": "OPEN",
        "regression": false
    })
}

fn finding_with(edit: impl FnOnce(&mut Value)) -> Finding {
    let mut v = finding_json();
    edit(&mut v);
    serde_json::from_value(v).expect("finding deserializes")
}

fn base_gates() -> Vec<Gate> {
    vec![
        gate("Tests", "required", "PASS"),
        gate("Build", "required", "PASS"),
    ]
}

fn base_findings() -> Vec<Finding> {
    vec![finding_with(|_| {})]
}

fn hash(gates: &[Gate], findings: &[Finding], ev: &[GateName]) -> String {
    determinism_inputs_hash(gates, findings, ev).expect("hashable")
}

/// AC-3: hand-written prefix / length / character-set check (no regex).
fn assert_hash_format(h: &str) {
    assert!(h.starts_with("sha256:"), "missing prefix: {h}");
    assert_eq!(h.len(), 71, "wrong length: {h}");
    assert!(
        h.as_bytes()[7..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)),
        "non-lowercase-hex digest: {h}"
    );
}

// ---- AC-1: SHA-256 wiring (FIPS 180-2 vectors) -----------------------------

#[test]
fn ac1_sha256_reference_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    assert_eq!(
        sha256_hex(&vec![b'a'; 1_000_000]),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

// ---- AC-2: empty golden ----------------------------------------------------

#[test]
fn ac2_empty_golden() {
    const EMPTY_ENVELOPE: &[u8] =
        br#"{"expectation_violations":[],"findings":[],"gates":[],"scheme":"fira-dih/1"}"#;
    const EMPTY_HASH: &str =
        "sha256:d8b9b41db41794bad872083cda6694df2785184f11b06105e93755ce13220b4b";
    // The pinned bytes hash to the pinned value…
    assert_eq!(
        format!("{DETERMINISM_HASH_PREFIX}{}", sha256_hex(EMPTY_ENVELOPE)),
        EMPTY_HASH
    );
    // …and the public function produces exactly that value for empty inputs.
    // (The byte-exact envelope is additionally asserted in-module.)
    assert_eq!(hash(&[], &[], &[]), EMPTY_HASH);
    assert_eq!(DETERMINISM_HASH_SCHEME, "fira-dih/1");
}

// ---- drift guard: non-empty golden (design §3.7) ---------------------------

/// Expected canonical bytes written by hand from design §3.4 (sorted keys,
/// escaped `"`, raw UTF-8 `é`, absent `cause`/`unverified_remainder`); the hash
/// was derived once from these bytes while drafting the test.
#[test]
fn nonempty_golden_drift_guard() {
    const EXPECTED_BYTES: &str = concat!(
        r#"{"expectation_violations":["Build","Tests"],"findings":[],"gates":[{"#,
        r#""name":"Tests","rationale":"r \"q\" é","requirement_level":"required","#,
        r#""state":"PASS","support_mappings":[{"claim":"c","evidence_refs":["make-test"],"#,
        r#""relevant_span":"Makefile:3","sufficiency":"SUFFICIENT"}],"#,
        r#""supporting_findings":[]}],"scheme":"fira-dih/1"}"#
    );
    const EXPECTED_HASH: &str =
        "sha256:29a583f82ff09bbe7cd11cf543c4125e40fc8f10bab26492158212a9a2366586";
    assert_eq!(
        format!("sha256:{}", sha256_hex(EXPECTED_BYTES.as_bytes())),
        EXPECTED_HASH
    );

    let g = gate_with(|v| v["rationale"] = json!("r \"q\" é"));
    let ev = [GateName::Tests, GateName::Build, GateName::Tests];
    assert_eq!(hash(&[g], &[], &ev), EXPECTED_HASH);
}

// ---- AC-3: format ----------------------------------------------------------

#[test]
fn ac3_hash_format() {
    assert_hash_format(&hash(&[], &[], &[]));
    assert_hash_format(&hash(&base_gates(), &base_findings(), &[GateName::Tests]));
    let (_, h) = assess_with_inputs_hash(&base_gates(), &base_findings(), &[]).unwrap();
    assert_hash_format(&h);
}

// ---- AC-4: determinism -----------------------------------------------------

#[test]
fn ac4_deterministic_including_serde_round_trip() {
    let gates = base_gates();
    let findings = base_findings();
    let first = hash(&gates, &findings, &[]);
    assert_eq!(first, hash(&gates, &findings, &[]));
    assert_eq!(first, hash(&gates.clone(), &findings.clone(), &[]));

    // Serialize → deserialize (as a persisted report.json would be read back).
    let gates_rt: Vec<Gate> =
        serde_json::from_str(&serde_json::to_string(&gates).unwrap()).unwrap();
    let findings_rt: Vec<Finding> =
        serde_json::from_str(&serde_json::to_string_pretty(&findings).unwrap()).unwrap();
    assert_eq!(first, hash(&gates_rt, &findings_rt, &[]));
}

// ---- AC-5: sensitivity -----------------------------------------------------

#[test]
fn ac5_any_single_input_change_changes_the_hash() {
    let findings = base_findings();
    let base = hash(&base_gates(), &findings, &[]);
    let other_build = gate("Build", "required", "PASS");

    let variants: Vec<(&str, String)> = vec![
        (
            "gate state",
            hash(
                &[
                    gate_with(|v| v["state"] = json!("FAIL")),
                    other_build.clone(),
                ],
                &findings,
                &[],
            ),
        ),
        (
            "gate rationale",
            hash(
                &[
                    gate_with(|v| v["rationale"] = json!("r2")),
                    other_build.clone(),
                ],
                &findings,
                &[],
            ),
        ),
        (
            "support-mapping sufficiency",
            hash(
                &[
                    gate_with(|v| v["support_mappings"][0]["sufficiency"] = json!("INSUFFICIENT")),
                    other_build.clone(),
                ],
                &findings,
                &[],
            ),
        ),
        (
            "finding field",
            hash(
                &base_gates(),
                &[finding_with(|v| v["title"] = json!("t2"))],
                &[],
            ),
        ),
        ("finding removed", hash(&base_gates(), &[], &[])),
        (
            "gate order",
            hash(
                &[other_build.clone(), gate("Tests", "required", "PASS")],
                &findings,
                &[],
            ),
        ),
        (
            "expectation set",
            hash(&base_gates(), &findings, &[GateName::Tests]),
        ),
    ];
    for (what, h) in &variants {
        assert_ne!(&base, h, "changing the {what} must change the hash");
    }
}

// ---- AC-7: expectation normalization + wrapper equivalence -----------------

#[test]
fn ac7_expectation_set_normalized() {
    let gates = base_gates();
    let findings = base_findings();
    let a = hash(&gates, &findings, &[GateName::Tests, GateName::Build]);
    let b = hash(&gates, &findings, &[GateName::Build, GateName::Tests]);
    let c = hash(
        &gates,
        &findings,
        &[GateName::Build, GateName::Tests, GateName::Build],
    );
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn ac7_wrapper_equals_direct_compute_assessment() {
    // A required PARTIAL gate is the only branch that reads the expectation set,
    // so the equivalence is exercised where it matters.
    let gates = vec![
        gate("Tests", "required", "PARTIAL"),
        gate("Build", "required", "PASS"),
    ];
    let findings = base_findings();
    let ev = [GateName::Tests, GateName::Build, GateName::Tests];

    let (assessment, h) = assess_with_inputs_hash(&gates, &findings, &ev).unwrap();
    let direct = compute_assessment(
        &gates,
        &findings,
        &ExpectationViolations::from_gates(ev.to_vec()),
    );
    assert_eq!(assessment, direct);
    assert_eq!(assessment.result, TechnicalAssessmentResult::NotReady);
    // The wrapper's hash is the hash of the same inputs.
    assert_eq!(h, hash(&gates, &findings, &ev));
}

// ---- AC-12 (b): `&[]` ≡ `ExpectationViolations::none()` ---------------------

#[test]
fn ac12b_empty_expectations_equal_none() {
    let gates = vec![
        gate("Tests", "required", "PARTIAL"),
        gate("Build", "required", "PASS"),
    ];
    let findings = base_findings();
    let (assessment, _) = assess_with_inputs_hash(&gates, &findings, &[]).unwrap();
    let direct = compute_assessment(&gates, &findings, &ExpectationViolations::none());
    assert_eq!(assessment, direct);
    // Required PARTIAL with no violation ⇒ RISK, not block (§11 else).
    assert_eq!(assessment.result, TechnicalAssessmentResult::ReadyWithRisks);
}

// ---- AC-8: UTC timestamp formatter (Task 4) --------------------------------

/// Fixed Unix-seconds inputs → expected UTC strings (requirements AC-8). No
/// clock is read and no external command is run.
#[test]
fn ac8_format_utc_timestamp_literal_table() {
    let table: [(u64, Option<&str>); 8] = [
        (0, Some("1970-01-01T00:00:00Z")),
        (951_782_400, Some("2000-02-29T00:00:00Z")),
        (1_767_225_599, Some("2025-12-31T23:59:59Z")),
        (4_102_444_800, Some("2100-01-01T00:00:00Z")),
        (4_107_542_399, Some("2100-02-28T23:59:59Z")),
        (4_107_542_400, Some("2100-03-01T00:00:00Z")),
        (253_402_300_799, Some("9999-12-31T23:59:59Z")),
        (253_402_300_800, None),
    ];
    for (secs, expected) in table {
        assert_eq!(
            format_utc_timestamp(secs).as_deref(),
            expected,
            "unix seconds {secs}"
        );
    }
}
