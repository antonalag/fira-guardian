//! ADAPTER/CLI tests: the §9.4 confirm-semantics matrix, output-path guard, and
//! an end-to-end minimal audit on a temp fixture project.

use std::fs;
use std::path::{Path, PathBuf};

use fira_adapters::cli::{run_audit, write_output, AuditArgs, Format, ProfileArg};
use fira_adapters::persistence::{resolve_default_base_with, OsFamily};
use fira_core::model::{GateName, GateState, ProfileId, TechnicalAssessmentResult};
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

/// Execution-grounded Tests + Build gate evaluation (vertical slice), end to end.
///
/// A `durable-agents`-shaped fixture: declared `test`/`build` mechanisms that
/// execute and pass, plus a declared watcher-like target. After the audit:
///   - Tests and Build gates are PASS (real execution evidence);
///   - every other profile gate stays UNKNOWN ("not sufficiently audited");
///   - findings remain empty (no P2–P8 synthesis in this slice);
///   - the verdict stays NOT_READY because other required gates are UNKNOWN (B4).
///
/// It uses a Makefile so the mechanisms run with `make` (no toolchain/network),
/// with `true` recipes so the run is deterministic and fast.
#[test]
fn execution_grounded_tests_build_gates_evaluate() {
    let dir = tempdir().unwrap();
    // `make test` / `make build` ⇒ discovered as `make-test` / `make-build`,
    // which back the Tests / Build gates; both run `true` ⇒ PASSED.
    fs::write(
        dir.path().join("Makefile"),
        "build:\n\ttrue\ntest:\n\ttrue\nlint:\n\ttrue\n",
    )
    .unwrap();

    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    // No manifest ⇒ classifier Undetermined; select a profile that requires
    // Tests + Build (human authority). cli_tool fits.
    a.profile = Some(ProfileArg::CliTool);
    let out = run_audit(a).expect("audit runs to completion");

    let gate = |name: GateName| {
        out.report
            .gates
            .iter()
            .find(|g| g.name == name)
            .unwrap_or_else(|| panic!("gate {name:?} missing from report"))
    };

    // Tests + Build evaluated to PASS from real execution evidence, each with a
    // SUFFICIENT support mapping (VR1).
    for name in [GateName::Tests, GateName::Build] {
        let g = gate(name);
        assert_eq!(
            g.state,
            GateState::Pass,
            "{name:?} should be PASS; got {:?}",
            g.state
        );
        assert!(
            !g.support_mappings.is_empty(),
            "{name:?} PASS must carry execution evidence"
        );
    }

    // Every other gate remains UNKNOWN (not sufficiently audited) — only
    // Tests/Build may change in this slice.
    for g in &out.report.gates {
        if g.name != GateName::Tests && g.name != GateName::Build {
            assert!(
                matches!(g.state, GateState::Unknown | GateState::NotApplicable),
                "non-slice gate {:?} must stay UNKNOWN/N/A; got {:?}",
                g.name,
                g.state
            );
        }
    }

    // Findings remain empty (no P2–P8 synthesis).
    assert!(
        out.report.findings.is_empty(),
        "no findings synthesized in this slice"
    );

    // The audited Tests/Build gates appear as coverage audited_areas (NO_ISSUE_FOUND).
    assert!(
        out.report
            .coverage
            .audited_areas
            .iter()
            .any(|a| a.area == "Tests"),
        "Tests must be an audited area"
    );

    // Verdict stays NOT_READY: cli_tool has other required gates still UNKNOWN
    // (B4), even though Tests/Build now PASS. Honest, not cosmetic.
    assert_eq!(
        out.report.technical_assessment.result,
        TechnicalAssessmentResult::NotReady,
        "other required gates are UNKNOWN ⇒ NOT_READY"
    );
}

/// A declared test mechanism that FAILS drives its gate to FAIL (never PASS).
#[test]
fn failing_test_mechanism_yields_fail_gate() {
    let dir = tempdir().unwrap();
    // `make test` runs `false` ⇒ FAILED; `make build` passes.
    fs::write(
        dir.path().join("Makefile"),
        "build:\n\ttrue\ntest:\n\tfalse\n",
    )
    .unwrap();

    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    a.profile = Some(ProfileArg::CliTool);
    let out = run_audit(a).expect("audit runs");

    let tests = out
        .report
        .gates
        .iter()
        .find(|g| g.name == GateName::Tests)
        .expect("Tests gate present");
    assert_eq!(
        tests.state,
        GateState::Fail,
        "a failing test mechanism ⇒ FAIL"
    );
    assert_ne!(tests.state, GateState::Pass);
}

// ---------------------------------------------------------------------------
// Single-property semantic audit slice: the JournalStore recovery-generation
// fencing invariant (Option A — UNVERIFIED ceiling). Three store-area fixtures.
// ---------------------------------------------------------------------------

/// Write a durable-agents-shaped store area. `with_guard` controls whether the
/// implementation carries the `recovery_generation` CAS guard (IMPLEMENTED), and
/// `with_fencing_test` whether the test body asserts fenced rejection (TESTED).
fn write_store_project(dir: &Path, with_guard: bool, with_fencing_test: bool) {
    fs::create_dir_all(dir.join("src/stores")).unwrap();
    fs::create_dir_all(dir.join("tests/stores")).unwrap();
    // A package.json so it is a real project (not required for the property read,
    // but keeps the fixture realistic).
    fs::write(
        dir.join("package.json"),
        r#"{ "name": "store-fixture", "scripts": { "build": "true", "test": "true" } }"#,
    )
    .unwrap();
    // The interface declares the fencing contract (SPECIFIED signal:
    // expectedGeneration + FENCED).
    fs::write(
        dir.join("src/stores/interface.ts"),
        "export interface JournalStore {\n  // Fenced: applies only while recovery_generation equals expectedGeneration;\n  // otherwise throws DurableError('FENCED').\n  updateRun(runId: string, updates: unknown, expectedGeneration: number): Promise<unknown>;\n}\n",
    )
    .unwrap();
    // The implementation guard (IMPLEMENTED signal: recovery_generation) present
    // only when with_guard.
    let impl_src = if with_guard {
        "export class SqliteJournalStore {\n  updateRun() {\n    // UPDATE runs SET ... WHERE run_id = ? AND recovery_generation = ?\n  }\n}\n"
    } else {
        // Guard removed (defect variant): the write does not check the generation.
        "export class SqliteJournalStore {\n  updateRun() {\n    // UPDATE runs SET ... WHERE run_id = ?\n  }\n}\n"
    };
    fs::write(dir.join("src/stores/sqlite.ts"), impl_src).unwrap();
    // The fencing test (TESTED signal: FENCED/fenced) present only when
    // with_fencing_test.
    let test_src = if with_fencing_test {
        "import { expect, it } from 'vitest';\nit('rejects a fenced terminal write', async () => {\n  await expect(store.updateRun(id, {}, staleGen)).rejects.toSatisfy(isFenced);\n});\n"
    } else {
        // Green suite but no fencing assertion exercised.
        "import { it, expect } from 'vitest';\nit('creates a run', async () => {\n  expect(await store.createRun({})).toBeDefined();\n});\n"
    };
    fs::write(dir.join("tests/stores/claim-recovery.test.ts"), test_src).unwrap();
}

/// AC-6 healthy baseline: declared + implemented + fencing test present. Under
/// Option A the property is UNVERIFIED (honest ceiling), the FailureRecovery gate
/// stays UNKNOWN (never PASS from this property), and the coverage records the
/// property's evidence state — with no spurious "missing"/"broken" claim.
#[test]
fn fencing_property_healthy_is_unverified_not_pass() {
    let dir = tempdir().unwrap();
    write_store_project(dir.path(), true, true);
    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    a.profile = Some(ProfileArg::StatefulDistributed);
    a.assume_yes = true;
    let out = run_audit(a).expect("audit runs");

    let fr = out
        .report
        .gates
        .iter()
        .find(|g| g.name == GateName::FailureRecovery)
        .expect("FailureRecovery gate present");
    // UNVERIFIED ⇒ the gate is never PASS from this property (C5).
    assert_ne!(
        fr.state,
        GateState::Pass,
        "fencing property must not drive PASS"
    );
    assert_eq!(
        fr.state,
        GateState::Unknown,
        "UNVERIFIED ⇒ gate stays UNKNOWN"
    );
    assert!(
        fr.rationale.contains("fencing invariant audited"),
        "gate rationale records the property audit; got {:?}",
        fr.rationale
    );

    // The property's evidence state is recorded as a coverage limitation, and it
    // honestly says UNVERIFIED (declared+implemented+tested, suite not proven to
    // exercise it).
    let note = out
        .report
        .coverage
        .known_limitations
        .iter()
        .find(|l| l.contains("fencing"))
        .expect("fencing property recorded in coverage");
    assert!(
        note.contains("UNVERIFIED"),
        "healthy baseline is UNVERIFIED; got {note}"
    );
    assert!(
        note.contains("SPECIFIED") && note.contains("IMPLEMENTED") && note.contains("TESTED"),
        "all three static facets reported; got {note}"
    );

    // No finding is published for an UNVERIFIED property (CONF-2).
    assert!(
        out.report.findings.is_empty(),
        "no published finding under Option A"
    );
}

/// AC-7 defect variant: the CAS guard is removed from the implementation. The
/// IMPLEMENTED facet drops; the property remains UNVERIFIED and never PASS, and
/// the report states the guard was not found in the inspected implementation.
#[test]
fn fencing_property_guard_removed_drops_implemented_facet() {
    let dir = tempdir().unwrap();
    write_store_project(dir.path(), false, true); // guard removed, test present
    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    a.profile = Some(ProfileArg::StatefulDistributed);
    a.assume_yes = true;
    let out = run_audit(a).expect("audit runs");

    let note = out
        .report
        .coverage
        .known_limitations
        .iter()
        .find(|l| l.contains("fencing"))
        .expect("fencing property recorded");
    assert!(
        note.contains("UNVERIFIED"),
        "guard-removed ⇒ UNVERIFIED; got {note}"
    );
    assert!(
        !note.contains("IMPLEMENTED"),
        "the guard is absent ⇒ no IMPLEMENTED facet; got {note}"
    );
    let fr = out
        .report
        .gates
        .iter()
        .find(|g| g.name == GateName::FailureRecovery)
        .unwrap();
    assert_ne!(fr.state, GateState::Pass);
}

/// AC-8 green-suite / not-exercised: the fencing assertion is absent (the suite
/// is otherwise green). The TESTED facet drops; a green suite never verifies the
/// property — it stays UNVERIFIED and never PASS.
#[test]
fn fencing_property_green_suite_without_fencing_test_is_unverified() {
    let dir = tempdir().unwrap();
    write_store_project(dir.path(), true, false); // guard present, no fencing test
    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    a.profile = Some(ProfileArg::StatefulDistributed);
    a.assume_yes = true;
    let out = run_audit(a).expect("audit runs");

    let note = out
        .report
        .coverage
        .known_limitations
        .iter()
        .find(|l| l.contains("fencing"))
        .expect("fencing property recorded");
    assert!(
        note.contains("UNVERIFIED"),
        "no fencing test ⇒ UNVERIFIED; got {note}"
    );
    assert!(
        !note.contains("TESTED"),
        "no fencing assertion ⇒ no TESTED facet; got {note}"
    );
    let fr = out
        .report
        .gates
        .iter()
        .find(|g| g.name == GateName::FailureRecovery)
        .unwrap();
    assert_ne!(
        fr.state,
        GateState::Pass,
        "a green suite never verifies the property"
    );
}

/// A non-store project (no src/stores/interface.ts) ⇒ the property audit is a
/// no-op: no fencing coverage note, and the FailureRecovery gate is untouched by
/// this slice (stays UNKNOWN from the base assembly).
#[test]
fn fencing_property_absent_on_non_store_project() {
    let dir = library_project(); // a Cargo lib; no src/stores
    let ws = tempdir().unwrap();
    let mut a = args_with_workspace(dir.path().to_path_buf(), ws.path());
    a.assume_yes = true;
    let out = run_audit(a).expect("audit runs");
    assert!(
        !out.report
            .coverage
            .known_limitations
            .iter()
            .any(|l| l.contains("fencing")),
        "no fencing audit on a non-store project"
    );
}
