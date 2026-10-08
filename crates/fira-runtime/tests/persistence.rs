//! RUNTIME persistence tests (WS-1, Task 11): the `WorkspaceSink` dual-containment
//! suite, atomic/partial-write behavior, identity/collision policy, crash
//! recovery, the fsync durability sequence (via a test seam), and the documented
//! TOCTOU boundary.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use fira_runtime::persistence::{FsyncObserver, OverwritePolicy, WorkspaceError, WorkspaceSink};
use tempfile::tempdir;

/// Create a `(base, project)` pair of sibling dirs (base is NOT inside project).
fn base_and_project() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let outer = tempdir().unwrap();
    let base = outer.path().join("workspace");
    let project = outer.path().join("project");
    fs::create_dir(&base).unwrap();
    fs::create_dir(&project).unwrap();
    (outer, base, project)
}

const JSON: &[u8] = br#"{"schema_version":"1.0.0"}"#;
const MD: &[u8] = b"# report\n";

fn pair() -> [(&'static str, &'static [u8]); 2] {
    [("report.json", JSON), ("report.md", MD)]
}

/// A persist writes report.json + report.md under <base>/<audit-id>/ and returns
/// the canonical location.
#[test]
fn persist_writes_pair_and_returns_location() {
    let (_outer, base, project) = base_and_project();
    let sink = WorkspaceSink::new(&base, &project).unwrap();
    let loc = sink
        .persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap();

    assert_eq!(loc, base.join("audit-1"));
    assert_eq!(fs::read(loc.join("report.json")).unwrap(), JSON);
    assert_eq!(fs::read(loc.join("report.md")).unwrap(), MD);
    // No stray staging/old sidecars remain.
    let leftovers: Vec<_> = fs::read_dir(&base)
        .unwrap()
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with(".staging-") || n.starts_with(".old-")
        })
        .collect();
    assert!(leftovers.is_empty(), "no sidecars left after publish");
}

// ---- Dual containment ------------------------------------------------------

/// Constructing with a base inside the project tree is rejected (AC-3).
#[test]
fn construct_rejects_base_inside_project() {
    let outer = tempdir().unwrap();
    let project = outer.path().join("project");
    let base = project.join("ws");
    fs::create_dir(&project).unwrap();
    fs::create_dir(&base).unwrap();
    assert_eq!(
        WorkspaceSink::new(&base, &project).unwrap_err(),
        WorkspaceError::WorkspaceInsideProject
    );
}

/// An unsafe audit_id (separators / `.` / `..` / absolute / empty) is rejected.
#[test]
fn rejects_unsafe_audit_id() {
    let (_outer, base, project) = base_and_project();
    let sink = WorkspaceSink::new(&base, &project).unwrap();
    for bad in ["a/b", "..", ".", "", "/abs", "a\0b"] {
        let err = sink
            .persist_artifacts(bad, &pair(), OverwritePolicy::CreateNew)
            .unwrap_err();
        assert!(
            matches!(err, WorkspaceError::UnsafeAuditId(_)),
            "id {bad:?} => {err:?}"
        );
    }
}

/// An unsafe file name is rejected the same way.
#[test]
fn rejects_unsafe_file_name() {
    let (_outer, base, project) = base_and_project();
    let sink = WorkspaceSink::new(&base, &project).unwrap();
    let arts: [(&str, &[u8]); 1] = [("../escape.json", JSON)];
    let err = sink
        .persist_artifacts("audit-1", &arts, OverwritePolicy::CreateNew)
        .unwrap_err();
    assert!(matches!(err, WorkspaceError::UnsafeAuditId(_)), "{err:?}");
}

/// A base that is a sibling of the project (string-prefix lookalike `/repo` vs
/// `/repo-2`) is accepted — component-wise containment, not string prefix.
#[test]
fn sibling_prefix_base_accepted() {
    let outer = tempdir().unwrap();
    let project = outer.path().join("repo");
    let base = outer.path().join("repo-2");
    fs::create_dir(&project).unwrap();
    fs::create_dir(&base).unwrap();
    let sink = WorkspaceSink::new(&base, &project).unwrap();
    let loc = sink
        .persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap();
    assert_eq!(loc, base.join("audit-1"));
}

/// A symlink inside the base pointing into the project tree, present at check
/// time, is rejected (containment re-check on the resolved target). Documents
/// the check-time guarantee (the TOCTOU boundary — §2.3).
#[cfg(unix)]
#[test]
fn symlink_into_project_rejected_at_check_time() {
    use std::os::unix::fs::symlink;
    let outer = tempdir().unwrap();
    let project = outer.path().join("project");
    let base = outer.path().join("workspace");
    fs::create_dir(&project).unwrap();
    fs::create_dir(&base).unwrap();
    // <base>/audit-1 is a symlink that resolves into the project tree.
    let target_in_project = project.join("sneaky");
    fs::create_dir(&target_in_project).unwrap();
    symlink(&target_in_project, base.join("audit-1")).unwrap();

    let sink = WorkspaceSink::new(&base, &project).unwrap();
    // The resolved symlink target lies outside the base AND inside the project
    // tree; the containment re-check rejects it before any publish. Either
    // `OutsideWorkspace` or `InsideProjectTree` is a correct rejection — the key
    // guarantee is that no project-tree write occurs.
    let err = sink
        .persist_artifacts("audit-1", &pair(), OverwritePolicy::ReplaceSameId)
        .unwrap_err();
    assert!(
        matches!(
            err,
            WorkspaceError::OutsideWorkspace | WorkspaceError::InsideProjectTree
        ),
        "symlink into project must be rejected on the resolved target, got {err:?}"
    );
    // The project tree gained no report files.
    assert!(!target_in_project.join("report.json").exists());
}

// ---- Identity / collision (§2.4) -------------------------------------------

/// CreateNew on an existing id fails with AuditIdExists (never clobbers).
#[test]
fn create_new_on_existing_id_fails() {
    let (_outer, base, project) = base_and_project();
    let sink = WorkspaceSink::new(&base, &project).unwrap();
    sink.persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap();
    let err = sink
        .persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap_err();
    assert_eq!(err, WorkspaceError::AuditIdExists("audit-1".to_string()));
}

/// ReplaceSameId replaces that id's artifacts; a different id is untouched.
#[test]
fn replace_same_id_replaces_only_that_id() {
    let (_outer, base, project) = base_and_project();
    let sink = WorkspaceSink::new(&base, &project).unwrap();

    sink.persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap();
    sink.persist_artifacts("audit-2", &pair(), OverwritePolicy::CreateNew)
        .unwrap();

    let new_json: &[u8] = br#"{"schema_version":"1.0.0","v":2}"#;
    let arts: [(&str, &[u8]); 2] = [("report.json", new_json), ("report.md", MD)];
    sink.persist_artifacts("audit-1", &arts, OverwritePolicy::ReplaceSameId)
        .unwrap();

    assert_eq!(
        fs::read(base.join("audit-1/report.json")).unwrap(),
        new_json
    );
    // audit-2 untouched.
    assert_eq!(fs::read(base.join("audit-2/report.json")).unwrap(), JSON);
}

// ---- Atomic publish / partial-write (§2.2) ---------------------------------

/// An fsync observer that fails on the Nth call — used to force a failure before
/// the publish rename, so the canonical dir is never created.
struct FailingFsync {
    fail_at: usize,
    calls: Rc<RefCell<usize>>,
}

impl FsyncObserver for FailingFsync {
    fn fsync_file(&self, _p: &Path) -> Result<(), WorkspaceError> {
        self.tick()
    }
    fn fsync_dir(&self, _p: &Path) -> Result<(), WorkspaceError> {
        self.tick()
    }
}

impl FailingFsync {
    fn tick(&self) -> Result<(), WorkspaceError> {
        let mut c = self.calls.borrow_mut();
        *c += 1;
        if *c >= self.fail_at {
            Err(WorkspaceError::WriteFailed("forced fsync failure".into()))
        } else {
            Ok(())
        }
    }
}

/// First-write failure during staging (before the publish rename) leaves NO
/// canonical dir and surfaces WriteFailed; the staging dir is cleaned.
#[test]
fn first_write_failure_leaves_no_canonical_dir() {
    let (_outer, base, project) = base_and_project();
    let calls = Rc::new(RefCell::new(0));
    let sink = WorkspaceSink::with_fsync(
        &base,
        &project,
        FailingFsync {
            fail_at: 1,
            calls: calls.clone(),
        },
    )
    .unwrap();

    let err = sink
        .persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap_err();
    assert!(matches!(err, WorkspaceError::WriteFailed(_)), "{err:?}");

    assert!(
        !base.join("audit-1").exists(),
        "no half-pair at canonical path"
    );
    // Staging cleaned up.
    let leftovers = fs::read_dir(&base)
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with(".staging-"));
    assert!(!leftovers, "staging dir cleaned after failure");
}

/// After a staging failure, a clean re-run succeeds (post-failure recovery).
#[test]
fn post_failure_rerun_succeeds() {
    let (_outer, base, project) = base_and_project();
    let calls = Rc::new(RefCell::new(0));
    {
        let sink = WorkspaceSink::with_fsync(
            &base,
            &project,
            FailingFsync {
                fail_at: 1,
                calls: calls.clone(),
            },
        )
        .unwrap();
        assert!(sink
            .persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
            .is_err());
    }
    // Clean sink succeeds.
    let sink = WorkspaceSink::new(&base, &project).unwrap();
    let loc = sink
        .persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap();
    assert!(loc.join("report.json").is_file());
}

// ---- Crash recovery between overwrite renames (§2.5) -----------------------

/// Simulate the between-renames state: the canonical dir is absent but a complete
/// `.old-*` sidecar survives. The next persist's recovery sweep restores a
/// complete canonical pair without manual steps.
#[test]
fn recovery_restores_from_old_sidecar() {
    let (_outer, base, project) = base_and_project();
    // Hand-build the interrupted state: audit-1 absent, a complete .old-* present.
    let old = base.join(".old-audit-1-deadbeef");
    fs::create_dir(&old).unwrap();
    fs::write(old.join("report.json"), JSON).unwrap();
    fs::write(old.join("report.md"), MD).unwrap();
    assert!(!base.join("audit-1").exists());

    let sink = WorkspaceSink::new(&base, &project).unwrap();
    // A fresh CreateNew persist runs the recovery sweep first, which restores the
    // canonical pair from the surviving .old-*; then — because the id now exists —
    // CreateNew reports the collision rather than clobbering the recovered data.
    let err = sink
        .persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap_err();
    assert_eq!(err, WorkspaceError::AuditIdExists("audit-1".to_string()));

    // The canonical pair was restored and the sidecar removed (restore-then-delete).
    assert_eq!(fs::read(base.join("audit-1/report.json")).unwrap(), JSON);
    assert!(!old.exists(), "old sidecar removed after confirmed restore");
}

/// Cleanup never deletes the only recoverable version: with the canonical dir
/// absent and only an INCOMPLETE `.staging-*` (missing report.md) plus a complete
/// `.old-*`, recovery restores from the complete `.old-*` (not the incomplete
/// staging) and the incomplete staging is discarded.
#[test]
fn recovery_prefers_complete_version() {
    let (_outer, base, project) = base_and_project();
    let incomplete = base.join(".staging-audit-1-aaaa");
    fs::create_dir(&incomplete).unwrap();
    fs::write(incomplete.join("report.json"), JSON).unwrap(); // missing report.md

    let old = base.join(".old-audit-1-bbbb");
    fs::create_dir(&old).unwrap();
    fs::write(old.join("report.json"), JSON).unwrap();
    fs::write(old.join("report.md"), MD).unwrap();

    let sink = WorkspaceSink::new(&base, &project).unwrap();
    let err = sink
        .persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap_err();
    assert_eq!(err, WorkspaceError::AuditIdExists("audit-1".to_string()));

    // Restored a COMPLETE pair, both sidecars gone.
    assert!(base.join("audit-1/report.json").is_file());
    assert!(base.join("audit-1/report.md").is_file());
    assert!(!incomplete.exists());
    assert!(!old.exists());
}

/// With no complete sidecar to restore from, the sweep leaves the sidecars in
/// place (never deletes the only potentially recoverable data).
#[test]
fn recovery_keeps_sidecars_when_none_complete() {
    let (_outer, base, project) = base_and_project();
    let incomplete = base.join(".staging-audit-9-cccc");
    fs::create_dir(&incomplete).unwrap();
    fs::write(incomplete.join("report.json"), JSON).unwrap(); // missing report.md

    let sink = WorkspaceSink::new(&base, &project).unwrap();
    // Persisting a DIFFERENT id triggers no recovery of audit-9; the incomplete
    // sidecar for audit-9 must remain untouched.
    sink.persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap();
    assert!(
        incomplete.exists(),
        "incomplete sidecar for another id preserved"
    );
}

// ---- Durability (§2.6) -----------------------------------------------------

/// A recording fsync observer: captures the sequence of (kind, path) fsync calls.
#[derive(Default)]
struct RecordingFsync {
    log: Rc<RefCell<Vec<(&'static str, PathBuf)>>>,
}

impl FsyncObserver for RecordingFsync {
    fn fsync_file(&self, p: &Path) -> Result<(), WorkspaceError> {
        self.log.borrow_mut().push(("file", p.to_path_buf()));
        Ok(())
    }
    fn fsync_dir(&self, p: &Path) -> Result<(), WorkspaceError> {
        self.log.borrow_mut().push(("dir", p.to_path_buf()));
        Ok(())
    }
}

/// The fsync sequence covers both files, the staging dir, and `<base>`
/// before/around publish (observable via the seam). Distinguishes durability
/// from rename atomicity; no overclaim about fsync-ignoring filesystems.
#[test]
fn fsync_sequence_covers_files_staging_and_base() {
    let (_outer, base, project) = base_and_project();
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink =
        WorkspaceSink::with_fsync(&base, &project, RecordingFsync { log: log.clone() }).unwrap();
    sink.persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap();

    let log = log.borrow();
    let files: Vec<_> = log.iter().filter(|(k, _)| *k == "file").collect();
    assert_eq!(files.len(), 2, "both artifact files fsynced");
    // A staging dir fsync and a base fsync both occur.
    let dir_syncs: Vec<_> = log.iter().filter(|(k, _)| *k == "dir").collect();
    assert!(dir_syncs.iter().any(|(_, p)| p
        .file_name()
        .map(|n| n.to_string_lossy().starts_with(".staging-"))
        .unwrap_or(false)));
    let base_canon = fs::canonicalize(&base).unwrap();
    assert!(dir_syncs.iter().any(|(_, p)| *p == base_canon));
    // Ordering: the base fsync is the last fsync (after the publish rename).
    assert_eq!(log.last().map(|(_, p)| p), Some(&base_canon));
}

// ---- Prior-report loader + project-untouched -------------------------------

/// The project directory is byte-for-byte unchanged after a persist (no
/// project-tree file is created).
#[test]
fn project_tree_unchanged_after_persist() {
    let (_outer, base, project) = base_and_project();
    fs::write(project.join("src.rs"), "fn main() {}").unwrap();
    let before = snapshot(&project);

    let sink = WorkspaceSink::new(&base, &project).unwrap();
    sink.persist_artifacts("audit-1", &pair(), OverwritePolicy::CreateNew)
        .unwrap();

    let after = snapshot(&project);
    assert_eq!(
        before, after,
        "project tree must be unchanged by persistence"
    );
}

/// Recursively snapshot (relative path, bytes) under a dir for equality checks.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    collect(dir, dir, &mut out);
    out.sort();
    out
}

fn collect(base: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect(base, &p, out);
        } else {
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
            out.push((rel, fs::read(&p).unwrap()));
        }
    }
}
