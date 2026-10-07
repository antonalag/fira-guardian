//! RUNTIME capability tests: reader confinement (READ-only) and OutputSink
//! path containment (WS-1/P-1), covering the approved boundary cases.

use std::fs;

use fira_core::interfaces::{CapabilityError, RepositoryReader};
use fira_runtime::{FsRepositoryReader, OutputPathError, OutputSink};
use tempfile::tempdir;

/// A reader reads files within the project root.
#[test]
fn reader_reads_within_root() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("file.txt"), "line1\nline2\nline3\n").unwrap();
    let reader = FsRepositoryReader::new(dir.path()).unwrap();

    assert_eq!(reader.read_file("file.txt", None).unwrap(), "line1\nline2\nline3\n");
    assert!(reader.resolve_ref("file.txt").unwrap());
    assert!(!reader.resolve_ref("missing.txt").unwrap());
}

/// A reader refuses to read outside the project root (confinement).
#[test]
fn reader_refuses_outside_root() {
    let outer = tempdir().unwrap();
    let project = outer.path().join("project");
    fs::create_dir(&project).unwrap();
    fs::write(outer.path().join("secret.txt"), "secret").unwrap();
    let reader = FsRepositoryReader::new(&project).unwrap();

    // Absolute escape and relative traversal both refused (NotPermitted/NotFound).
    let abs = outer.path().join("secret.txt");
    let err = reader.read_file(abs.to_str().unwrap(), None).unwrap_err();
    assert!(matches!(err, CapabilityError::NotPermitted | CapabilityError::NotFound(_)));

    let err2 = reader.read_file("../secret.txt", None).unwrap_err();
    assert!(matches!(err2, CapabilityError::NotPermitted | CapabilityError::NotFound(_)));
}

/// `OutputSink` rejects the project root itself.
#[test]
fn output_rejects_project_root() {
    let dir = tempdir().unwrap();
    let sink = OutputSink::new(dir.path()).unwrap();
    let err = sink.write(dir.path(), b"x").unwrap_err();
    assert_eq!(err, OutputPathError::IsProjectRoot);
}

/// `OutputSink` rejects a descendant of the project root (direct and nested).
#[test]
fn output_rejects_descendants() {
    let dir = tempdir().unwrap();
    let sink = OutputSink::new(dir.path()).unwrap();

    let direct = dir.path().join("report.json");
    assert_eq!(sink.write(&direct, b"x").unwrap_err(), OutputPathError::InsideProjectRoot);

    let nested = dir.path().join("sub/report.json");
    fs::create_dir(dir.path().join("sub")).unwrap();
    assert_eq!(sink.write(&nested, b"x").unwrap_err(), OutputPathError::InsideProjectRoot);
}

/// `OutputSink` rejects a `..` path that re-enters the project tree.
#[test]
fn output_rejects_traversal_reentry() {
    let dir = tempdir().unwrap();
    let sink = OutputSink::new(dir.path()).unwrap();
    let root_name = dir.path().file_name().unwrap().to_str().unwrap().to_string();
    // <root>/../<root-name>/report.json resolves back into the tree.
    let reenter = dir.path().join("..").join(&root_name).join("report.json");
    let err = sink.write(&reenter, b"x").unwrap_err();
    assert!(
        matches!(err, OutputPathError::InsideProjectRoot | OutputPathError::EscapesViaTraversalOrSymlink),
        "got {err:?}"
    );
}

/// `OutputSink` rejects a symlink whose target resolves inside the project tree.
#[cfg(unix)]
#[test]
fn output_rejects_symlink_into_tree() {
    use std::os::unix::fs::symlink;
    let outer = tempdir().unwrap();
    let project = outer.path().join("project");
    fs::create_dir(&project).unwrap();
    // A symlink outside the tree pointing INTO the tree.
    let link = outer.path().join("link-into-project");
    symlink(&project, &link).unwrap();
    let sink = OutputSink::new(&project).unwrap();

    // Writing "<link>/report.json" resolves (via the symlinked parent) into the
    // project tree and must be rejected.
    let via_link = link.join("report.json");
    let err = sink.write(&via_link, b"x").unwrap_err();
    assert!(
        matches!(err, OutputPathError::InsideProjectRoot | OutputPathError::EscapesViaTraversalOrSymlink),
        "got {err:?}"
    );
}

/// `OutputSink` accepts a destination outside the project tree and writes it.
#[test]
fn output_accepts_outside_tree() {
    let outer = tempdir().unwrap();
    let project = outer.path().join("project");
    fs::create_dir(&project).unwrap();
    let sink = OutputSink::new(&project).unwrap();

    let outside = outer.path().join("report.json");
    let written = sink.write(&outside, b"hello").unwrap();
    assert_eq!(fs::read_to_string(&written).unwrap(), "hello");
}

/// A `/repo` root must not falsely reject a `/repo-2`-style sibling (component-
/// wise containment, not string prefix).
#[test]
fn output_sibling_prefix_not_false_rejected() {
    let outer = tempdir().unwrap();
    let repo = outer.path().join("repo");
    let repo2 = outer.path().join("repo-2");
    fs::create_dir(&repo).unwrap();
    fs::create_dir(&repo2).unwrap();
    let sink = OutputSink::new(&repo).unwrap();

    let dest = repo2.join("report.json");
    let written = sink.write(&dest, b"ok").unwrap();
    assert_eq!(fs::read_to_string(&written).unwrap(), "ok");
}
