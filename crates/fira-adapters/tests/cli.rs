//! ADAPTER/CLI tests: the §9.4 confirm-semantics matrix, output-path guard, and
//! an end-to-end minimal audit on a temp fixture project.

use std::fs;
use std::path::PathBuf;

use fira_adapters::cli::{run_audit, write_output, AuditArgs, Format, ProfileArg};
use fira_core::model::ProfileId;
use tempfile::tempdir;

fn args(project: PathBuf) -> AuditArgs {
    AuditArgs {
        project,
        release_target: "v1".to_string(),
        profile: None,
        depth: None,
        assume_yes: false,
        format: Format::Both,
        output: None,
    }
}

/// A library-shaped Cargo project ⇒ classifier proposes `library`.
fn library_project() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"demo\"\n",
    )
    .unwrap();
    dir
}

/// An empty project ⇒ classifier Undetermined.
fn empty_project() -> tempfile::TempDir {
    tempdir().unwrap()
}

/// Classified proposal + --yes ⇒ explicitly confirms the proposed profile.
#[test]
fn classified_plus_yes_confirms() {
    let dir = library_project();
    let mut a = args(dir.path().to_path_buf());
    a.assume_yes = true;
    let out = run_audit(a).expect("audit runs");
    assert_eq!(out.applied.applied, ProfileId::Library);
}

/// Classified proposal, non-interactive, no --yes ⇒ fail (no implicit accept).
#[test]
fn classified_without_yes_fails() {
    let dir = library_project();
    let a = args(dir.path().to_path_buf());
    assert!(run_audit(a).is_err(), "must not auto-accept without explicit confirmation");
}

/// Undetermined + no --profile ⇒ fail clearly.
#[test]
fn undetermined_without_profile_fails() {
    let dir = empty_project();
    let a = args(dir.path().to_path_buf());
    assert!(run_audit(a).is_err(), "undetermined must fail without --profile");
}

/// Undetermined + explicit --profile ⇒ human selects one of the five.
#[test]
fn undetermined_with_profile_selects() {
    let dir = empty_project();
    let mut a = args(dir.path().to_path_buf());
    a.profile = Some(ProfileArg::WebService);
    let out = run_audit(a).expect("explicit profile selection runs");
    assert_eq!(out.applied.applied, ProfileId::WebService);
}

/// --profile overrides a Classified proposal (explicit human reclassification).
#[test]
fn profile_overrides_proposal() {
    let dir = library_project();
    let mut a = args(dir.path().to_path_buf());
    a.profile = Some(ProfileArg::CliTool);
    let out = run_audit(a).expect("runs");
    assert_eq!(out.applied.applied, ProfileId::CliTool);
}

/// End-to-end: a minimal run yields a schema-valid-shaped, honest report.
#[test]
fn end_to_end_minimal_report() {
    let dir = library_project();
    let mut a = args(dir.path().to_path_buf());
    a.assume_yes = true;
    let out = run_audit(a).expect("runs");

    // Applied profile is the confirmed class; the report records its gates.
    assert_eq!(out.report.applied_profile.profile_id, ProfileId::Library);
    assert!(!out.report.gates.is_empty());
    // Honest coverage: the deferred-interpretation limitation is recorded.
    assert!(!out.report.coverage.known_limitations.is_empty());
    // Verdict is computed by the CORE engine (computed_by_rule is set).
    assert!(!out.report.technical_assessment.computed_by_rule.is_empty());
}

/// The output guard rejects a path inside the project tree (WS-1/P-1) and
/// accepts an outside path.
#[test]
fn output_guard_rejects_inside_accepts_outside() {
    let outer = tempdir().unwrap();
    let project = outer.path().join("project");
    fs::create_dir(&project).unwrap();

    // Inside the tree ⇒ rejected.
    let inside = project.join("report.json");
    assert!(write_output(&project, &inside, b"x").is_err());

    // Outside the tree ⇒ accepted.
    let outside = outer.path().join("report.json");
    let written = write_output(&project, &outside, b"x").expect("outside path accepted");
    assert_eq!(fs::read_to_string(written).unwrap(), "x");
}
