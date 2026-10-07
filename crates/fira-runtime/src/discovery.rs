//! Discovery of **project-declared** verification mechanisms (EXEC-1, §6).
//!
//! Discovery reads structural manifests only and records what the project
//! already declares — it **never synthesizes** a command. Each discovered entry
//! is `declared_by_project=true` (S12). The parsed `command` is split into an
//! explicit argument vector at execution time (no shell).
//!
//! MVP sources: `Cargo.toml` (a `test`/`build` convention), `package.json`
//! (`scripts`), and `Makefile` (targets). The set is intentionally small and
//! extended only by adding more structural parsers — never by inventing commands.

use std::fs;
use std::path::Path;

use fira_core::execution::VerificationMechanism;
use fira_core::model::{AlwaysTrue, IdRef, VerificationMechanismSource};

/// A discovered mechanism plus the exact argv to run it with (no shell). The
/// argv is kept alongside the S12 record so the executor never re-parses or
/// shells out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredMechanism {
    pub mechanism: VerificationMechanism,
    /// Explicit argument vector (program + args). Never passed to a shell.
    pub argv: Vec<String>,
}

/// Detect structural classification signals (§12 P1) from the project tree.
/// Structural manifests only — presence and manifest-declared target roles; it
/// reads no README/doc/narrative content. Lives in RUNTIME because it touches
/// the filesystem; CORE's classifier consumes the returned signals.
pub fn detect_signals(project_root: &Path) -> fira_core::classification::ClassificationSignals {
    use fira_core::classification::ClassificationSignals;
    let exists = |rel: &str| project_root.join(rel).exists();
    let mut s = ClassificationSignals {
        has_cargo_toml: exists("Cargo.toml"),
        has_package_json: exists("package.json"),
        has_pom_xml: exists("pom.xml"),
        has_build_gradle: exists("build.gradle"),
        has_go_mod: exists("go.mod"),
        has_dockerfile: exists("Dockerfile"),
        has_ci_config: exists(".github") || exists(".gitlab-ci.yml") || exists(".circleci"),
        ..Default::default()
    };

    // Cargo target roles (structural, from the manifest only).
    if s.has_cargo_toml {
        if let Ok(text) = fs::read_to_string(project_root.join("Cargo.toml")) {
            if let Ok(v) = toml::from_str::<toml::Value>(&text) {
                if v.get("lib").is_some() {
                    s.declares_library_target = true;
                }
                if v.get("bin").is_some() {
                    s.declares_binary_target = true;
                }
            }
        }
    }

    // package.json "bin" field declares a CLI entrypoint (structural).
    if s.has_package_json {
        if let Ok(text) = fs::read_to_string(project_root.join("package.json")) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                if v.get("bin").is_some() {
                    s.declares_binary_target = true;
                }
            }
        }
    }

    s
}

/// Discover project-declared mechanisms under `project_root`. Pure structural
/// parsing of manifests; no execution, no synthesis.
pub fn discover(project_root: &Path) -> Vec<DiscoveredMechanism> {
    let mut found = Vec::new();
    discover_cargo(project_root, &mut found);
    discover_package_json(project_root, &mut found);
    discover_makefile(project_root, &mut found);
    found
}

fn mk(
    id: &str,
    source: VerificationMechanismSource,
    source_locator: &str,
    command: &str,
    argv: Vec<&str>,
) -> DiscoveredMechanism {
    DiscoveredMechanism {
        mechanism: VerificationMechanism {
            id: IdRef(id.to_string()),
            source,
            source_locator: source_locator.to_string(),
            command: command.to_string(),
            declared_by_project: AlwaysTrue,
        },
        argv: argv.into_iter().map(str::to_string).collect(),
    }
}

/// A `Cargo.toml` declares a Cargo project; `cargo test` / `cargo build` are the
/// project's declared verification/build mechanisms for that manifest. These are
/// Cargo-declared conventions, not synthesized commands.
fn discover_cargo(root: &Path, out: &mut Vec<DiscoveredMechanism>) {
    let manifest = root.join("Cargo.toml");
    if !manifest.is_file() {
        return;
    }
    let Ok(text) = fs::read_to_string(&manifest) else {
        return;
    };
    // Confirm it parses as a Cargo manifest (has a [package] or [workspace]).
    let parsed: Result<toml::Value, _> = toml::from_str(&text);
    let is_cargo = matches!(&parsed, Ok(v) if v.get("package").is_some() || v.get("workspace").is_some());
    if !is_cargo {
        return;
    }
    let loc = "Cargo.toml:1";
    out.push(mk(
        "cargo-build",
        VerificationMechanismSource::Other,
        loc,
        "cargo build",
        vec!["cargo", "build"],
    ));
    out.push(mk(
        "cargo-test",
        VerificationMechanismSource::Other,
        loc,
        "cargo test",
        vec!["cargo", "test"],
    ));
}

/// `package.json` `scripts` entries are project-declared mechanisms, run via
/// `npm run <name>` (the project declared the script; npm is the declared
/// runner). Only the script *names* the project wrote are discovered.
fn discover_package_json(root: &Path, out: &mut Vec<DiscoveredMechanism>) {
    let manifest = root.join("package.json");
    if !manifest.is_file() {
        return;
    }
    let Ok(text) = fs::read_to_string(&manifest) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(scripts) = value.get("scripts").and_then(|s| s.as_object()) else {
        return;
    };
    // Deterministic order.
    let mut names: Vec<&String> = scripts.keys().collect();
    names.sort();
    for name in names {
        // Skip anything that is not a plain script string.
        if !scripts.get(name).map(|v| v.is_string()).unwrap_or(false) {
            continue;
        }
        out.push(mk(
            &format!("npm-{name}"),
            VerificationMechanismSource::Npm,
            "package.json",
            &format!("npm run {name}"),
            vec!["npm", "run", name],
        ));
    }
}

/// `Makefile` targets are project-declared mechanisms, run via `make <target>`.
/// Only targets the project wrote are discovered (structural line scan).
fn discover_makefile(root: &Path, out: &mut Vec<DiscoveredMechanism>) {
    let manifest = root.join("Makefile");
    if !manifest.is_file() {
        return;
    }
    let Ok(text) = fs::read_to_string(&manifest) else {
        return;
    };
    for (i, line) in text.lines().enumerate() {
        // A target line looks like `name:` or `name: deps` at column 0, not a
        // recipe (recipes are tab-indented) and not a variable assignment.
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((lhs, _)) = line.split_once(':') else {
            continue;
        };
        let target = lhs.trim();
        if target.is_empty()
            || target.contains('=')
            || target.contains(' ')
            || target.starts_with('.')
        {
            continue;
        }
        out.push(mk(
            &format!("make-{target}"),
            VerificationMechanismSource::Make,
            &format!("Makefile:{}", i + 1),
            &format!("make {target}"),
            vec!["make", target],
        ));
    }
}
