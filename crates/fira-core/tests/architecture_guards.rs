//! Task 1 direction-guard tests (structural boundaries).
//!
//! These prove the intended layer boundaries hold at the manifest and source
//! level. Per `design.md` §6.1, the source-scan guard is **defense-in-depth
//! only**; the primary enforcement of WS-1/EXEC-1 is the architectural/
//! dependency boundary (proven by `manifest_test`) and, in later tasks, runtime
//! capability enforcement.
//!
//! Run from the `fira-core` package; paths are resolved relative to the
//! workspace root, discovered by walking up from CARGO_MANIFEST_DIR.

use std::path::{Path, PathBuf};

/// Workspace root = parent of `crates/`. `CARGO_MANIFEST_DIR` points at
/// `<root>/crates/fira-core`, so go up two levels.
fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent() // <root>/crates
        .and_then(Path::parent) // <root>
        .expect("workspace root two levels above fira-core manifest dir")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

fn parse_manifest(crate_name: &str) -> toml::Value {
    let root = workspace_root();
    let manifest = root.join("crates").join(crate_name).join("Cargo.toml");
    read(&manifest)
        .parse::<toml::Value>()
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", manifest.display()))
}

/// Dependency names declared under `[dependencies]` for a crate.
fn runtime_dependency_names(crate_name: &str) -> Vec<String> {
    let value = parse_manifest(crate_name);
    match value.get("dependencies") {
        Some(toml::Value::Table(t)) => t.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

const SIBLING_CRATES: [&str; 6] = [
    "fira-core",
    "fira-policy",
    "fira-runtime",
    "fira-presentation",
    "fira-adapters",
    "fira-cli",
];

/// AC-5: `fira-core` has no `[dependencies]` entry naming a sibling crate.
/// This is the *primary* proof that CORE is platform-independent and cannot
/// perform project writes or process execution (it does not depend on the layer
/// that will own those capabilities).
#[test]
fn core_has_no_sibling_dependencies() {
    let deps = runtime_dependency_names("fira-core");
    for dep in &deps {
        assert!(
            !SIBLING_CRATES.contains(&dep.as_str()),
            "fira-core must not depend on sibling crate `{dep}` (CORE is platform-independent)"
        );
    }
}

/// AC-6: `fira-policy` depends at most on `fira-core`.
#[test]
fn policy_depends_at_most_on_core() {
    let deps = runtime_dependency_names("fira-policy");
    for dep in &deps {
        if SIBLING_CRATES.contains(&dep.as_str()) {
            assert_eq!(
                dep, "fira-core",
                "fira-policy may only depend on fira-core among siblings, found `{dep}`"
            );
        }
    }
}

/// AC-8: contract version alignment across the marker file and CORE constant.
#[test]
fn contract_version_is_consistent() {
    let root = workspace_root();
    let marker = read(&root.join("CONTRACT_VERSION"));
    assert_eq!(marker.trim(), fira_core::CONTRACT_VERSION);
    assert_eq!(fira_core::CONTRACT_VERSION, "1.0-frozen+corr1-4");
}

/// AC-9: the frozen-contract doc records the four corrections via their
/// invariant IDs.
#[test]
fn frozen_contract_doc_contains_correction_invariants() {
    let root = workspace_root();
    let doc = read(&root.join("docs/contract/frozen-mvp-contract.md"));
    for needle in ["E1", "CONF-2", "EXEC-1", "WS-1"] {
        assert!(
            doc.contains(needle),
            "frozen-mvp-contract.md must reference invariant `{needle}` (correction traceability)"
        );
    }
}

/// AC-10 (defense-in-depth): `fira-core` and `fira-policy` sources contain no
/// filesystem-write or process-spawn calls. Not the primary WS-1/EXEC-1
/// enforcement — a tripwire that catches accidental capability leakage into
/// layers that must not own it.
#[test]
fn core_and_policy_sources_have_no_fs_write_or_process() {
    let root = workspace_root();
    let forbidden = ["std::fs::write", "std::fs::File::create", "std::process::"];
    for crate_name in ["fira-core", "fira-policy"] {
        let src = root.join("crates").join(crate_name).join("src");
        for file in rust_sources(&src) {
            let contents = read(&file);
            for pattern in forbidden {
                assert!(
                    !contents.contains(pattern),
                    "{} must not reference `{pattern}` (defense-in-depth guard for WS-1/EXEC-1)",
                    file.display()
                );
            }
        }
    }
}

/// Collect `*.rs` files under a directory tree.
fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("failed to read dir {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            out.extend(rust_sources(&path));
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path);
        }
    }
    out
}
