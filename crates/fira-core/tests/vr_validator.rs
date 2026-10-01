//! AC-4, AC-6, AC-7.
//!
//! - AC-6: the VR validator rejects each semantic invalid fixture with a
//!   rule-identifying error (the error names the VR).
//! - AC-7: named invalid fixtures for VR11 / VR12 / VR13 are rejected.
//! - AC-4: the golden report is accepted (no violations).

mod common;

use common::load_json;
use fira_core::validation::{run_vrs, StaticValidationContext, Vr};

fn violations_for(fixture: &str) -> Vec<fira_core::validation::Violation> {
    let report = load_json(&format!("schemas/examples/invalid/{fixture}"));
    let ctx = StaticValidationContext::from_report(&report);
    run_vrs(&report, &ctx)
}

/// AC-4: the golden composite report passes every semantic VR (no violations).
#[test]
fn golden_report_has_no_violations() {
    let report = load_json("schemas/examples/valid/s11-audit-report.json");
    let ctx = StaticValidationContext::from_report(&report);
    let violations = run_vrs(&report, &ctx);
    assert!(
        violations.is_empty(),
        "golden report must pass all semantic VRs, got: {violations:?}"
    );
}

/// Assert that a fixture is rejected and that the expected VR is named among the
/// violations.
fn assert_rejected_by(fixture: &str, expected: Vr) {
    let violations = violations_for(fixture);
    assert!(
        !violations.is_empty(),
        "{fixture} must be rejected by the validator"
    );
    assert!(
        violations.iter().any(|v| v.vr == expected),
        "{fixture} must be rejected by {} (got {:?})",
        expected.id(),
        violations
    );
}

#[test]
fn vr01_rejected() {
    assert_rejected_by("vr01-no-sufficient-mapping.json", Vr::Vr1);
}

#[test]
fn vr02_rejected() {
    assert_rejected_by("vr02-recovery-missing-anchor.json", Vr::Vr2);
}

#[test]
fn vr08_rejected() {
    assert_rejected_by("vr08-gate-without-coverage.json", Vr::Vr8);
}

#[test]
fn vr09_rejected() {
    assert_rejected_by("vr09-safe-language.json", Vr::Vr9);
}

/// AC-7: VR11 named fixture (static-only VERIFIED) is rejected.
#[test]
fn vr11_rejected() {
    assert_rejected_by("vr11-static-only-verified.json", Vr::Vr11);
}

/// AC-7: VR12 named fixture (synthesized command) is rejected.
#[test]
fn vr12_rejected() {
    assert_rejected_by("vr12-synthesized-command.json", Vr::Vr12);
}

/// AC-7: VR13 named fixture (write under project_root) is rejected.
#[test]
fn vr13_rejected() {
    assert_rejected_by("vr13-write-under-project-root.json", Vr::Vr13);
}

/// VR5/VR7 are surface-only in Task 2: the only checkable-now fixture is the
/// CONF-2/VR1 overlap (a published finding with no SUFFICIENT mapping), which is
/// rejected by VR1 — not by any confidence recomputation.
#[test]
fn vr05_surface_overlaps_vr1_only() {
    let violations = violations_for("vr05-surface-insufficient-finding.json");
    assert!(
        violations.iter().any(|v| v.vr == Vr::Vr1),
        "VR5 surface fixture is checkable now only via the VR1/CONF-2 overlap, got: {violations:?}"
    );
}

/// Each named correction fixture is rejected by exactly its own rule (no
/// accidental cross-triggering that would blur which rule fired).
#[test]
fn correction_fixtures_are_single_rule() {
    for (fixture, expected) in [
        ("vr11-static-only-verified.json", Vr::Vr11),
        ("vr12-synthesized-command.json", Vr::Vr12),
        ("vr13-write-under-project-root.json", Vr::Vr13),
    ] {
        let violations = violations_for(fixture);
        let distinct: std::collections::HashSet<Vr> =
            violations.iter().map(|v| v.vr).collect();
        assert_eq!(
            distinct.len(),
            1,
            "{fixture} should trip exactly one rule, got {violations:?}"
        );
        assert!(distinct.contains(&expected));
    }
}
