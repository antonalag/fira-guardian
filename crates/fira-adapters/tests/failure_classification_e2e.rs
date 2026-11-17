//! End-to-end tests for the failure-classification slice (Task 5).
//!
//! Each test runs the real `run_audit` over a throwaway project and checks the
//! resulting report structurally (fields of `report.gates` / `report.coverage`,
//! never substring searches over the whole JSON).
//!
//! Only shapes that `run_audit` can really produce are exercised. FAILED or
//! ERROR with APP_LEVEL, FAILED with INFRA_LEVEL, ERROR with UNCLASSIFIED and an
//! `Err` from the executor cannot come out of real discovery and execution (the
//! discovered launchers are `cargo`/`npm`/`make`, the project root exists, and
//! the executor registry is built from the iterated collection). They are
//! covered by the CORE tables (Task 2), the RUNTIME tests (Task 3) and the
//! stub-based ADAPTER unit tests (Task 4), not here.
//!
//! Environment: `PATH` is process-global and `Command` reads it at spawn time,
//! so EVERY test in this binary takes the same lock, sets `PATH` explicitly and
//! restores it on drop. `PATH` is only ever set to an existing, empty temporary
//! directory or to its original value, never to the empty string (an empty
//! component of `PATH` means the current directory, i.e. the audited project).
//! Fixture Makefiles contain only `true` / `false` recipes. Nothing here uses
//! the network.
//!
//! Scope note: `e5_*` is additional integration coverage authorized for this
//! task. It is not an acceptance criterion of the spec.
//!
//! Known limitations L-18 and L-19 are accepted and deferred: no assertion here
//! reads or depends on the `claim` of a support mapping.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use fira_adapters::cli::{run_audit, AuditArgs, AuditOutput, Format, ProfileArg};
use fira_core::gate::Gate;
use fira_core::model::{
    CoverageResult, ExecutionOutcome, GateCause, GateName, GateState, Sufficiency,
    TechnicalAssessmentResult,
};
use serde_json::Value;
use tempfile::{tempdir, TempDir};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Holds the environment lock and restores `PATH` when dropped (the restore runs
/// before the lock field is released).
struct EnvGuard {
    saved: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn lock() -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        EnvGuard {
            saved: std::env::var_os("PATH"),
            _lock: lock,
        }
    }

    /// `PATH` = `dir`, an existing, empty directory: no launcher can be found.
    fn empty_path(dir: &Path) -> Self {
        let guard = Self::lock();
        std::env::set_var("PATH", dir);
        guard
    }

    /// `PATH` set explicitly to its original value, and `make` must be on it.
    /// A missing `make` is a failed precondition, not a skipped test.
    fn original_path_with_make() -> Self {
        let guard = Self::lock();
        let original = guard
            .saved
            .clone()
            .expect("precondition failed: PATH is not set, so `make` cannot be found");
        let has_make = std::env::split_paths(&original).any(|d| d.join("make").is_file());
        assert!(
            has_make,
            "precondition failed: `make` is not available on PATH; the scenarios that run \
             Makefile mechanisms need it (they would otherwise report ERROR/INFRA_LEVEL \
             instead of the outcome under test)"
        );
        std::env::set_var("PATH", &original);
        guard
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.saved {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
    }
}

// ---------------------------------------------------------------------------
// Fixtures and helpers
// ---------------------------------------------------------------------------

fn project_with(files: &[(&str, &str)]) -> TempDir {
    let dir = tempdir().unwrap();
    for (name, body) in files {
        fs::write(dir.path().join(name), body).unwrap();
    }
    dir
}

/// Makefile with the two gate-backing targets and one non-backing target.
const MAKEFILE_ALL_TRUE: &str = "build:\n\ttrue\ntest:\n\ttrue\nlint:\n\ttrue\n";
const MAKEFILE_TEST_FALSE: &str = "build:\n\ttrue\ntest:\n\tfalse\n";
/// `npm run test` whose declared body is a dev server: classified NOT_RUN
/// without launching `npm`.
const PACKAGE_JSON_WATCH_TEST: &str = "{\"scripts\":{\"test\":\"vite dev\"}}";

/// Every entry under `root` (relative path, directory flag, file bytes), sorted.
fn tree_snapshot(root: &Path) -> Vec<(String, bool, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, bool, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                out.push((rel, true, Vec::new()));
                walk(root, &path, out);
            } else {
                out.push((rel, false, fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn args(project: &Path, workspace: &Path) -> AuditArgs {
    AuditArgs {
        project: project.to_path_buf(),
        release_target: "v1".to_string(),
        // `cli_tool` requires Build and Tests; an explicit profile avoids the
        // confirmation path.
        profile: Some(ProfileArg::CliTool),
        depth: None,
        assume_yes: true,
        format: Format::Both,
        output: None,
        workspace: Some(workspace.to_path_buf()),
    }
}

/// Run the real audit. `run_audit` returning `Ok` means `validate_report`
/// (VR1/5/7/8/9/11/12/13) found no violation; an `Err` (including
/// `VrViolations`) is reported verbatim and is never worked around.
/// The project tree must be identical before and after.
fn audit(project: &Path) -> (AuditOutput, TempDir) {
    let ws = tempdir().unwrap();
    let before = tree_snapshot(project);
    let out = run_audit(args(project, ws.path()))
        .unwrap_or_else(|e| panic!("run_audit must return Ok for this report shape: {e}"));
    assert_eq!(
        before,
        tree_snapshot(project),
        "the audited project tree must not change"
    );
    (out, ws)
}

fn gate(out: &AuditOutput, name: GateName) -> &Gate {
    out.report
        .gates
        .iter()
        .find(|g| g.name == name)
        .unwrap_or_else(|| panic!("gate {name:?} missing from the report"))
}

fn wire(name: GateName) -> String {
    serde_json::to_value(name)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

fn outcome_of(out: &AuditOutput, id: &str) -> ExecutionOutcome {
    out.report
        .coverage
        .executed_mechanisms
        .iter()
        .find(|r| r.command_id.0 == id)
        .unwrap_or_else(|| panic!("no executed_mechanisms row for {id}"))
        .outcome
}

fn limitation_for<'a>(out: &'a AuditOutput, id: &str) -> &'a String {
    out.report
        .coverage
        .known_limitations
        .iter()
        .find(|l| l.contains(&format!("'{id}'")))
        .unwrap_or_else(|| panic!("no limitation naming {id}"))
}

/// Each gate appears in at least one coverage list, and a gate represented in
/// `blocked_areas` appears in no other list (exactly one of the three).
fn assert_each_gate_in_exactly_one_list(out: &AuditOutput) {
    let cov = &out.report.coverage;
    for g in &out.report.gates {
        let name = wire(g.name);
        let in_audited = cov.audited_areas.iter().any(|a| a.area == name);
        let in_skipped = cov.skipped_areas.iter().any(|a| a.area == name);
        let in_blocked = cov.blocked_areas.iter().any(|a| a.area == name);
        assert_eq!(
            [in_audited, in_skipped, in_blocked]
                .iter()
                .filter(|b| **b)
                .count(),
            1,
            "gate {name} must be in exactly one coverage list \
             (audited={in_audited}, skipped={in_skipped}, blocked={in_blocked})"
        );
    }
}

fn no_gate_has_app_cause(out: &AuditOutput) {
    for g in &out.report.gates {
        assert_ne!(
            g.cause,
            Some(GateCause::App),
            "gate {:?} must not carry an APP attribution",
            g.name
        );
    }
}

/// E1 fixture run: every launcher is missing (`PATH` = empty directory).
fn infra_audit() -> (AuditOutput, TempDir, TempDir) {
    let project = project_with(&[("Makefile", MAKEFILE_ALL_TRUE)]);
    let empty = tempdir().unwrap();
    let _env = EnvGuard::empty_path(empty.path());
    let (out, ws) = audit(project.path());
    drop(_env);
    (out, ws, project)
}

// ---------------------------------------------------------------------------
// E1 (AC-15, AC-17): launcher missing ⇒ ERROR/INFRA rows, UNKNOWN/INFRA gates,
// `blocked_areas` only for backed gates
// ---------------------------------------------------------------------------

#[test]
fn e1_missing_launcher_records_error_rows_and_blocked_areas() {
    let (out, _ws, _project) = infra_audit();
    let cov = &out.report.coverage;

    // Rows: every mechanism produced a result, each ERROR (not dropped).
    for id in ["make-build", "make-test", "make-lint"] {
        assert_eq!(outcome_of(&out, id), ExecutionOutcome::Error, "{id}");
    }

    // The backed gates are UNKNOWN with an infrastructure cause (never PASS or
    // FAIL), and every mapping rests on insufficient evidence.
    for name in [GateName::Build, GateName::Tests] {
        let g = gate(&out, name);
        assert_eq!(g.state, GateState::Unknown, "{name:?}");
        assert_eq!(g.cause, Some(GateCause::Infra), "{name:?}");
        assert!(!g.support_mappings.is_empty());
        for m in &g.support_mappings {
            assert_eq!(m.sufficiency, Sufficiency::Insufficient, "{name:?}");
            assert!(m.unverified_remainder.is_some(), "{name:?}");
        }
    }
    no_gate_has_app_cause(&out);

    // blocked_areas: exactly the two backed gates, named after the gate.
    let areas: BTreeSet<&str> = cov.blocked_areas.iter().map(|b| b.area.as_str()).collect();
    assert_eq!(areas, BTreeSet::from(["Build", "Tests"]));
    assert_eq!(cov.blocked_areas.len(), 2, "one entry per backed gate here");
    let gate_names: BTreeSet<String> = out.report.gates.iter().map(|g| wire(g.name)).collect();
    for b in &cov.blocked_areas {
        assert!(
            gate_names.contains(&b.area),
            "area {} is not a gate",
            b.area
        );
        assert!(!b.blocking_condition.is_empty());
        assert!(
            cov.executed_mechanisms
                .iter()
                .any(|r| r.command_id == b.execution_result_ref),
            "dangling execution_result_ref {:?}",
            b.execution_result_ref
        );
    }
    // The reference of each entry is a mechanism that backs that gate.
    let by_area = |a: &str| {
        cov.blocked_areas
            .iter()
            .find(|b| b.area == a)
            .unwrap()
            .execution_result_ref
            .0
            .clone()
    };
    assert_eq!(by_area("Build"), "make-build");
    assert_eq!(by_area("Tests"), "make-test");

    // The non-backing mechanism gets its row and note but no coverage entry.
    assert!(cov
        .blocked_areas
        .iter()
        .all(|b| b.execution_result_ref.0 != "make-lint"));

    // Blocked gates are not skipped; the others stay skipped.
    for blocked in ["Build", "Tests"] {
        assert!(!cov.skipped_areas.iter().any(|s| s.area == blocked));
    }
    assert!(!cov.skipped_areas.is_empty(), "other gates remain skipped");
    assert_each_gate_in_exactly_one_list(&out);

    // Notes: one per ERROR mechanism, hedged, and no application attribution.
    for id in ["make-build", "make-test", "make-lint"] {
        let note = limitation_for(&out, id);
        assert!(
            note.contains("could not be started (outcome ERROR, INFRA_LEVEL)"),
            "{note}"
        );
        assert!(note.contains("exact cause is not established"), "{note}");
        assert!(!note.contains("APP_LEVEL"), "{note}");
    }

    // Required gates UNKNOWN ⇒ not ready; no finding is synthesized.
    assert_eq!(
        out.report.technical_assessment.result,
        TechnicalAssessmentResult::NotReady
    );
    assert!(out.report.findings.is_empty());
}

// ---------------------------------------------------------------------------
// E2 (AC-15, persistence): the persisted report.json carries the same shape
// ---------------------------------------------------------------------------

#[test]
fn e2_persisted_report_contains_error_rows_and_blocked_areas() {
    let (out, _ws, project) = infra_audit();

    let loc = PathBuf::from(&out.canonical_location);
    assert!(
        !loc.starts_with(project.path()),
        "the report is persisted outside the project tree"
    );
    let text = fs::read_to_string(loc.join("report.json")).expect("report.json is persisted");

    // Round trip: what is persisted is the report that was validated.
    let parsed = fira_presentation::parse_json(&text).expect("persisted report parses");
    assert_eq!(parsed, out.report);

    // Structural checks on the persisted JSON.
    let v: Value = serde_json::from_str(&text).unwrap();
    let rows = v["coverage"]["executed_mechanisms"].as_array().unwrap();
    for id in ["make-build", "make-test", "make-lint"] {
        assert!(
            rows.iter()
                .any(|r| r["command_id"] == id && r["outcome"] == "ERROR"),
            "persisted ERROR row for {id} missing"
        );
    }
    let blocked = v["coverage"]["blocked_areas"].as_array().unwrap();
    assert_eq!(blocked.len(), 2);
    for (area, id) in [("Build", "make-build"), ("Tests", "make-test")] {
        let entry = blocked
            .iter()
            .find(|b| b["area"] == area)
            .unwrap_or_else(|| panic!("persisted blocked area {area} missing"));
        assert_eq!(entry["execution_result_ref"], id);
        assert!(!entry["blocking_condition"].as_str().unwrap().is_empty());
    }
    let skipped = v["coverage"]["skipped_areas"].as_array().unwrap();
    for blocked_gate in ["Build", "Tests"] {
        assert!(!skipped.iter().any(|s| s["area"] == blocked_gate));
    }
    let gates = v["gates"].as_array().unwrap();
    for name in ["Build", "Tests"] {
        let g = gates.iter().find(|g| g["name"] == name).unwrap();
        assert_eq!(g["state"], "UNKNOWN");
        assert_eq!(g["cause"], "INFRA");
    }
}

// ---------------------------------------------------------------------------
// E3 (AC-17, AC-18): a failing test target is FAIL with no application claim
// ---------------------------------------------------------------------------

#[test]
fn e3_failing_test_target_is_fail_without_application_attribution() {
    let project = project_with(&[("Makefile", MAKEFILE_TEST_FALSE)]);
    let _env = EnvGuard::original_path_with_make();
    let (out, _ws) = audit(project.path());

    assert_eq!(outcome_of(&out, "make-test"), ExecutionOutcome::Failed);
    assert_eq!(outcome_of(&out, "make-build"), ExecutionOutcome::Passed);

    let tests = gate(&out, GateName::Tests);
    assert_eq!(tests.state, GateState::Fail);
    assert_eq!(tests.cause, Some(GateCause::None));
    for phrase in [
        "attribution of the failure",
        "is not established",
        "no application defect has been established",
    ] {
        assert!(
            tests.rationale.contains(phrase),
            "{phrase}: {}",
            tests.rationale
        );
    }
    assert!(!tests.rationale.to_lowercase().contains("app-level"));
    assert!(!tests.rationale.contains("APP_LEVEL"));
    no_gate_has_app_cause(&out);

    // The sibling Build gate is unaffected by the failing Tests mechanism.
    assert_eq!(gate(&out, GateName::Build).state, GateState::Pass);

    // The FAILED note states what is and is not established.
    let note = limitation_for(&out, "make-test");
    for phrase in [
        "FAILED",
        "not yet interpreted",
        "attribution is not established",
        "no application defect has been established",
    ] {
        assert!(note.contains(phrase), "{phrase}: {note}");
    }
    for l in &out.report.coverage.known_limitations {
        assert!(!l.contains("APP_LEVEL"), "{l}");
    }

    // Documents L-11: the failing gate is an audited area with ISSUES_FOUND.
    let cov = &out.report.coverage;
    let tests_area = cov
        .audited_areas
        .iter()
        .find(|a| a.area == "Tests")
        .expect("Tests is an audited area");
    assert_eq!(tests_area.result, CoverageResult::IssuesFound);

    // FAILED/UNCLASSIFIED is not an infrastructure impediment.
    assert!(cov.blocked_areas.is_empty());
    assert_each_gate_in_exactly_one_list(&out);
}

// ---------------------------------------------------------------------------
// E4 (AC-19): NOT_RUN without launching npm
// ---------------------------------------------------------------------------

#[test]
fn e4_not_run_mechanism_is_not_launched_and_not_blocked() {
    let project = project_with(&[("package.json", PACKAGE_JSON_WATCH_TEST)]);
    // No launcher exists on PATH: had `npm` been started, the row would be ERROR.
    let empty = tempdir().unwrap();
    let _env = EnvGuard::empty_path(empty.path());
    let (out, _ws) = audit(project.path());

    assert_eq!(outcome_of(&out, "npm-test"), ExecutionOutcome::NotRun);
    let note = limitation_for_command(&out, "npm run test");
    assert!(note.contains("not executed"), "{note}");

    let tests = gate(&out, GateName::Tests);
    assert_eq!(tests.state, GateState::Unknown);
    assert_eq!(tests.cause, Some(GateCause::None));

    let cov = &out.report.coverage;
    assert!(cov.blocked_areas.is_empty(), "NOT_RUN never blocks (L-3)");
    assert!(cov.skipped_areas.iter().any(|s| s.area == "Tests"));
    assert_each_gate_in_exactly_one_list(&out);
    no_gate_has_app_cause(&out);
}

/// The coverage limitation for a command line (NOT_RUN / TIMEOUT reasons are
/// prefixed with the command, not with the mechanism id).
fn limitation_for_command<'a>(out: &'a AuditOutput, command: &str) -> &'a String {
    out.report
        .coverage
        .known_limitations
        .iter()
        .find(|l| l.starts_with(&format!("{command}: ")))
        .unwrap_or_else(|| panic!("no limitation for command {command}"))
}

// ---------------------------------------------------------------------------
// E5 (additional integration coverage, not a spec acceptance criterion):
// a PASSED mechanism cannot hide an inconclusive one backing the same gate
// ---------------------------------------------------------------------------

#[test]
fn e5_passed_plus_not_run_on_same_gate_is_never_pass() {
    let project = project_with(&[
        ("package.json", PACKAGE_JSON_WATCH_TEST),
        ("Makefile", "test:\n\ttrue\n"),
    ]);
    let _env = EnvGuard::original_path_with_make();
    let (out, _ws) = audit(project.path());

    assert_eq!(outcome_of(&out, "npm-test"), ExecutionOutcome::NotRun);
    assert_eq!(outcome_of(&out, "make-test"), ExecutionOutcome::Passed);

    let tests = gate(&out, GateName::Tests);
    assert_eq!(tests.state, GateState::Unknown, "never PASS");
    assert_eq!(tests.cause, Some(GateCause::None));

    // Sufficiency per backing mechanism, identified by evidence ref only.
    let sufficiency_of = |id: &str| {
        let m = tests
            .support_mappings
            .iter()
            .find(|m| m.evidence_refs.iter().any(|r| r.0 == id))
            .unwrap_or_else(|| panic!("no mapping for {id}"));
        m.sufficiency
    };
    assert_eq!(sufficiency_of("make-test"), Sufficiency::Sufficient);
    assert_eq!(sufficiency_of("npm-test"), Sufficiency::Insufficient);

    assert!(out.report.coverage.blocked_areas.is_empty());
    assert_each_gate_in_exactly_one_list(&out);
}

// ---------------------------------------------------------------------------
// E6 (AC-24): determinism
// ---------------------------------------------------------------------------

#[test]
fn e6_infra_report_is_deterministic_across_runs() {
    let project = project_with(&[("Makefile", MAKEFILE_ALL_TRUE)]);
    let empty = tempdir().unwrap();
    let _env = EnvGuard::empty_path(empty.path());
    let (a, _ws_a) = audit(project.path());
    let (b, _ws_b) = audit(project.path());

    assert_ne!(
        a.report.audit_id, b.report.audit_id,
        "guard: the two runs are distinct audits"
    );
    assert_eq!(a.report.gates, b.report.gates);
    assert_eq!(
        a.report.coverage.blocked_areas,
        b.report.coverage.blocked_areas
    );
    assert_eq!(
        a.report.coverage.known_limitations,
        b.report.coverage.known_limitations
    );
    assert_eq!(
        a.report.determinism_inputs_hash,
        b.report.determinism_inputs_hash
    );
    assert!(!a.report.coverage.blocked_areas.is_empty(), "non-vacuous");
}

// ---------------------------------------------------------------------------
// E7 (AC-23, e2e part): the audited project tree is never written
// ---------------------------------------------------------------------------

#[test]
fn e7_project_tree_is_unchanged_by_infra_and_failed_audits() {
    // INFRA: nothing can be launched.
    let infra_project = project_with(&[("Makefile", MAKEFILE_ALL_TRUE)]);
    {
        let empty = tempdir().unwrap();
        let _env = EnvGuard::empty_path(empty.path());
        let before = tree_snapshot(infra_project.path());
        assert!(!before.is_empty(), "guard: the snapshot is not vacuous");
        let (out, _ws) = audit(infra_project.path());
        assert_eq!(before, tree_snapshot(infra_project.path()));
        assert!(!PathBuf::from(&out.canonical_location).starts_with(infra_project.path()));
    }

    // FAILED: a mechanism really runs (`false`).
    let failed_project = project_with(&[("Makefile", MAKEFILE_TEST_FALSE)]);
    {
        let _env = EnvGuard::original_path_with_make();
        let before = tree_snapshot(failed_project.path());
        let (out, _ws) = audit(failed_project.path());
        assert_eq!(before, tree_snapshot(failed_project.path()));
        assert!(!PathBuf::from(&out.canonical_location).starts_with(failed_project.path()));
    }
}
