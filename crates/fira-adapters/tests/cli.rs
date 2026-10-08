//! ADAPTER/CLI tests: the §9.4 confirm-semantics matrix, output-path guard, and
//! an end-to-end minimal audit on a temp fixture project.

use std::fs;
use std::path::{Path, PathBuf};

use fira_adapters::cli::{run_audit, write_output, AuditArgs, Format, ProfileArg};
use fira_adapters::persistence::{resolve_default_base_with, OsFamily};
use fira_core::model::ProfileId;
use tempfile::tempdir;

/// Build audit args with an explicit workspace base (outside the project tree)
/// so the persist-by-default run (§9.4) has a canonical location.
fn args_with_workspace(project: PathBuf, workspace: &Path) -> AuditArgs {
    AuditArgs {
        project,
        release_target: "v1".to_string(),
        profile: None,
        depth: None,
        assume_yes: false,
        format: Format::Both,
        output: None,
        workspace: Some(workspace.to_path_buf()),
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
    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    a.assume_yes = true;
    let out = run_audit(a).expect("audit runs");
    assert_eq!(out.applied.applied, ProfileId::Library);
}

/// Classified proposal, non-interactive, no --yes ⇒ fail (no implicit accept).
#[test]
fn classified_without_yes_fails() {
    let dir = library_project();
    let ws = tempdir().unwrap();
    let a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    assert!(
        run_audit(a).is_err(),
        "must not auto-accept without explicit confirmation"
    );
}

/// Undetermined + no --profile ⇒ fail clearly.
#[test]
fn undetermined_without_profile_fails() {
    let dir = empty_project();
    let ws = tempdir().unwrap();
    let a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    assert!(
        run_audit(a).is_err(),
        "undetermined must fail without --profile"
    );
}

/// Undetermined + explicit --profile ⇒ human selects one of the five.
#[test]
fn undetermined_with_profile_selects() {
    let dir = empty_project();
    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    a.profile = Some(ProfileArg::WebService);
    let out = run_audit(a).expect("explicit profile selection runs");
    assert_eq!(out.applied.applied, ProfileId::WebService);
}

/// --profile overrides a Classified proposal (explicit human reclassification).
#[test]
fn profile_overrides_proposal() {
    let dir = library_project();
    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    a.profile = Some(ProfileArg::CliTool);
    let out = run_audit(a).expect("runs");
    assert_eq!(out.applied.applied, ProfileId::CliTool);
}

/// End-to-end: a minimal run yields a schema-valid-shaped, honest report.
#[test]
fn end_to_end_minimal_report() {
    let dir = library_project();
    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
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

/// End-to-end persist (§9.4): a run writes the canonical pair under
/// `<base>/<audit-id>/` and returns that location; the JSON round-trips; the
/// stdout/`--output` copy is a distinct exported artifact.
#[test]
fn end_to_end_persists_canonical_pair() {
    let dir = library_project();
    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    a.assume_yes = true;
    let out = run_audit(a).expect("runs");

    let loc = PathBuf::from(&out.canonical_location);
    let json_path = loc.join("report.json");
    let md_path = loc.join("report.md");
    assert!(json_path.is_file(), "report.json at canonical location");
    assert!(md_path.is_file(), "report.md at canonical location");

    // Canonical location is inside the workspace base, keyed by the audit id.
    assert_eq!(
        loc.file_name().unwrap().to_string_lossy(),
        out.report.audit_id
    );

    // The persisted JSON round-trips back to an equal report.
    let text = fs::read_to_string(&json_path).unwrap();
    let parsed = fira_presentation::parse_json(&text).expect("round-trips");
    assert_eq!(parsed.audit_id, out.report.audit_id);

    // The report pair is NOT inside the project tree (dual containment).
    assert!(!loc.starts_with(dir.path()));
}

/// A `--workspace` that resolves inside the project tree is rejected (dual
/// containment at construction, AC-3) — persistence never collapses into a
/// project write.
#[test]
fn workspace_inside_project_is_rejected() {
    let dir = library_project();
    let inside = dir.path().join("ws");
    fs::create_dir(&inside).unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), &inside);
    a.assume_yes = true;
    assert!(
        run_audit(a).is_err(),
        "a workspace base inside the project tree must be rejected"
    );
    // And nothing was written under the project-tree workspace candidate.
    let empty = fs::read_dir(&inside).unwrap().next().is_none();
    assert!(
        empty,
        "no canonical artifacts written inside the project tree"
    );
}

/// The data-dir resolver honors the per-OS candidate order and the
/// absent/empty/relative fallbacks (design §4.1), using injected env lookups so
/// the real process environment is never mutated.
#[test]
fn resolver_candidate_order_and_fallbacks() {
    // Linux: XDG_DATA_HOME wins when set + absolute.
    let base = resolve_default_base_with(OsFamily::Unix, &|k| match k {
        "XDG_DATA_HOME" => Some("/data/xdg".to_string()),
        "HOME" => Some("/home/u".to_string()),
        _ => None,
    })
    .expect("resolves");
    assert!(base.starts_with("/data/xdg"));
    assert!(base.ends_with("fira-guardian/fira-workspace"));

    // Linux: empty XDG + absolute HOME ⇒ fall through to $HOME/.local/share.
    let base = resolve_default_base_with(OsFamily::Unix, &|k| match k {
        "XDG_DATA_HOME" => Some("   ".to_string()),
        "HOME" => Some("/home/u".to_string()),
        _ => None,
    })
    .expect("resolves");
    assert!(base.starts_with("/home/u/.local/share"));

    // Linux: relative XDG is rejected (not resolved against cwd); HOME used.
    let base = resolve_default_base_with(OsFamily::Unix, &|k| match k {
        "XDG_DATA_HOME" => Some("relative/dir".to_string()),
        "HOME" => Some("/home/u".to_string()),
        _ => None,
    })
    .expect("resolves");
    assert!(base.starts_with("/home/u/.local/share"));

    // Linux: nothing qualifies ⇒ error asking for --workspace.
    let err = resolve_default_base_with(OsFamily::Unix, &|_| None).unwrap_err();
    assert!(err.contains("--workspace"), "got: {err}");

    // macOS: HOME-based.
    let base = resolve_default_base_with(OsFamily::MacOs, &|k| match k {
        "HOME" => Some("/Users/u".to_string()),
        _ => None,
    })
    .expect("resolves");
    assert!(base.starts_with("/Users/u/Library/Application Support"));

    // Windows: LOCALAPPDATA first, then APPDATA, then USERPROFILE.
    let base = resolve_default_base_with(OsFamily::Windows, &|k| match k {
        "APPDATA" => Some("C:\\Users\\u\\AppData\\Roaming".to_string()),
        "USERPROFILE" => Some("C:\\Users\\u".to_string()),
        _ => None, // LOCALAPPDATA unset
    })
    .expect("resolves");
    assert!(base.to_string_lossy().contains("Roaming"));
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

/// The prior-report loader (§9.3) reads a persisted `report.json`, parses it, and
/// returns it as historical input (C12); a missing path yields `None`.
#[test]
fn prior_report_loader_reads_and_structurally_accepts() {
    use fira_adapters::persistence::load_prior_report;

    // Persist a report, then load it back via the read-only loader.
    let dir = library_project();
    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    a.assume_yes = true;
    let out = run_audit(a).expect("runs");

    let json_path = PathBuf::from(&out.canonical_location).join("report.json");
    let loaded = load_prior_report(&json_path).expect("loads").expect("some");
    assert_eq!(loaded.audit_id, out.report.audit_id);

    // A missing path is None, not an error.
    let missing = ws.path().join("nope/report.json");
    assert!(load_prior_report(&missing).unwrap().is_none());
}
