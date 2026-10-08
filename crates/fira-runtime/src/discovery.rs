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
    /// Task 14: `Some(kind)` when the declared command is structurally
    /// non-terminating (a watcher / dev server / REPL) and therefore must **not**
    /// be executed as a one-shot verification mechanism. The mechanism still
    /// appears in the registry as project evidence; the executor returns a
    /// `NOT_RUN` result for it. `None` ⇒ executable-for-verification (the default;
    /// the execution bound is the backstop for an unexpected hang).
    ///
    /// This is a **runtime-local** field: it is never serialized into the S12
    /// `VerificationMechanism` record (which stays `additionalProperties:false`),
    /// so no schema changes.
    pub non_terminating_reason: Option<String>,
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
    let argv: Vec<String> = argv.into_iter().map(str::to_string).collect();
    // Task 14: classify from the declared command text (conservative; §3).
    let non_terminating_reason = classify_non_terminating(command, &argv);
    DiscoveredMechanism {
        mechanism: VerificationMechanism {
            id: IdRef(id.to_string()),
            source,
            source_locator: source_locator.to_string(),
            command: command.to_string(),
            declared_by_project: AlwaysTrue,
        },
        argv,
        non_terminating_reason,
    }
}

/// Classify a discovered mechanism as structurally non-terminating (Task 14,
/// design §3). Pure and deterministic: a function of the already-declared
/// command text / argv only — it executes nothing and reads nothing new.
///
/// **Conservative:** returns `Some(kind)` only on an enumerated, explicit signal
/// (a watcher flag, a watch subcommand, or a known dev-server/REPL program).
/// Anything not matching is treated as executable-for-verification (`None`); the
/// runtime execution bound is the backstop for an unexpected hang. It never uses
/// name similarity or fuzzy heuristics, and it is never used for deduplication.
/// The declared **command text is authoritative** over any script name.
pub(crate) fn classify_non_terminating(command: &str, argv: &[String]) -> Option<String> {
    // Tokenize on whitespace for flag detection; keep argv for program/subcmd.
    let tokens: Vec<String> = if argv.is_empty() {
        command.split_whitespace().map(str::to_string).collect()
    } else {
        // For `npm run <name>` the argv is the runner, not the script body; the
        // declared `command` string is the human-written command, so scan both.
        let mut t: Vec<String> = argv.to_vec();
        t.extend(command.split_whitespace().map(str::to_string));
        t
    };
    let lower: Vec<String> = tokens.iter().map(|t| t.to_ascii_lowercase()).collect();
    let has = |needle: &str| lower.iter().any(|t| t == needle);
    let program_basename = |p: &str| -> String {
        p.rsplit(['/', '\\'])
            .next()
            .unwrap_or(p)
            .to_ascii_lowercase()
    };

    // (1) Explicit watch flags on any runner (vitest/jest/tsc/webpack/rollup/…).
    //     A standalone `--watch` or `-w`, or a `watch` subcommand token.
    if has("--watch") || has("-w") || has("watch") {
        return Some("watcher (declared --watch/-w/watch)".to_string());
    }
    // `node --watch <script>` is also a watcher.
    // (covered by the --watch check above)

    // (2) Known dev-server / watcher / REPL programs by basename.
    //     The program is argv[0] when present, else the first command token.
    let program = argv
        .first()
        .cloned()
        .or_else(|| command.split_whitespace().next().map(str::to_string))
        .unwrap_or_default();
    let prog = program_basename(&program);

    // Dev servers / watchers that run until interrupted.
    const WATCHER_PROGRAMS: &[&str] = &["nodemon", "webpack-dev-server"];
    if WATCHER_PROGRAMS.contains(&prog.as_str()) {
        return Some(format!("dev server/watcher ({prog})"));
    }

    // Runner subcommands that start a long-lived server: `vite dev`, `vite serve`,
    // `vite preview`, `next dev`, `next start`, `webpack serve`, `nuxt dev`, etc.
    let subcmd = argv.get(1).map(|s| s.to_ascii_lowercase()).or_else(|| {
        command
            .split_whitespace()
            .nth(1)
            .map(|s| s.to_ascii_lowercase())
    });
    let is_server_subcmd = matches!(
        (prog.as_str(), subcmd.as_deref()),
        ("vite", Some("dev"))
            | ("vite", Some("serve"))
            | ("vite", Some("preview"))
            | ("next", Some("dev"))
            | ("next", Some("start"))
            | ("nuxt", Some("dev"))
            | ("webpack", Some("serve"))
            | ("astro", Some("dev"))
            | ("remix", Some("dev"))
    );
    if is_server_subcmd {
        return Some(format!(
            "dev server ({prog} {})",
            subcmd.unwrap_or_default()
        ));
    }

    // A bare `vite` / `next` / `nuxt` with no subcommand defaults to a dev server.
    if matches!(prog.as_str(), "vite" | "next" | "nuxt") && subcmd.is_none() {
        return Some(format!("dev server ({prog}, no subcommand)"));
    }

    // (3) REPL-like invocations: an interpreter with no script/args. Running
    //     these would block reading stdin forever (also handled by stdin
    //     isolation, but we decline to execute them for verification regardless).
    const REPL_PROGRAMS: &[&str] = &["node", "python", "python3", "irb", "ghci", "deno"];
    if REPL_PROGRAMS.contains(&prog.as_str()) {
        // Only a *bare* interpreter (no further argument) is a REPL. `deno` is a
        // special case: `deno` alone is a REPL, but `deno run …`/`deno test …`
        // are one-shot — so require no args for all of these.
        let has_further_arg = argv.len() > 1 || command.split_whitespace().count() > 1;
        if !has_further_arg {
            return Some(format!("REPL ({prog} with no script)"));
        }
    }

    None
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
    let is_cargo =
        matches!(&parsed, Ok(v) if v.get("package").is_some() || v.get("workspace").is_some());
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
        let Some(body) = scripts.get(name).and_then(|v| v.as_str()) else {
            continue;
        };
        let mut m = mk(
            &format!("npm-{name}"),
            VerificationMechanismSource::Npm,
            "package.json",
            &format!("npm run {name}"),
            vec!["npm", "run", name],
        );
        // Task 14: the mechanism is *run* via `npm run <name>`, but its
        // declared **body** (the script value in package.json) is the authoritative
        // structural text for the non-terminating classification — e.g. a `test:watch`
        // script whose body is `vitest --watch`. The body is project-declared text
        // (no execution, no synthesis), so classifying from it stays within scope.
        if m.non_terminating_reason.is_none() {
            m.non_terminating_reason = classify_non_terminating(
                body,
                &body
                    .split_whitespace()
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            );
        }
        out.push(m);
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

#[cfg(test)]
mod classifier_tests {
    //! Task 14: the non-terminating classifier is pure and deterministic. Unit
    //! tests live here because `classify_non_terminating` is crate-internal.

    use super::classify_non_terminating;

    fn argv(cmd: &str) -> Vec<String> {
        cmd.split_whitespace().map(str::to_string).collect()
    }

    /// Non-terminating commands are flagged (watchers / dev servers / REPLs).
    #[test]
    fn flags_non_terminating_commands() {
        for cmd in [
            "vitest --watch",
            "vitest watch",
            "jest --watch",
            "tsc --watch",
            "tsc -w",
            "webpack --watch",
            "nodemon src/index.js",
            "vite dev",
            "vite serve",
            "next dev",
            "next start",
            "nuxt dev",
            "webpack serve",
            "vite",
            "node",
            "python",
        ] {
            assert!(
                classify_non_terminating(cmd, &argv(cmd)).is_some(),
                "expected {cmd:?} to be classified non-terminating"
            );
        }
    }

    /// Ordinary one-shot verification commands are NOT flagged.
    #[test]
    fn allows_one_shot_commands() {
        for cmd in [
            "vitest run",
            "vitest run --coverage",
            "cargo test",
            "cargo build",
            "eslint src/ tests/",
            "tsc --noEmit",
            "make test",
            "node dist/build.js",
            "deno run build.ts",
            "deno test",
            "npm run build",
        ] {
            assert!(
                classify_non_terminating(cmd, &argv(cmd)).is_none(),
                "expected {cmd:?} to be executable-for-verification (not flagged)"
            );
        }
    }

    /// Deterministic: same input ⇒ same output.
    #[test]
    fn is_deterministic() {
        let cmd = "vitest --watch";
        let a = classify_non_terminating(cmd, &argv(cmd));
        let b = classify_non_terminating(cmd, &argv(cmd));
        assert_eq!(a, b);
    }

    /// Command text is authoritative over script name: the classifier sees the
    /// `npm run <name>` wrapper as one-shot (the body is hidden at this level);
    /// `discover_package_json` is what feeds the *body* to the classifier, so a
    /// `test:watch` whose body is `vitest --watch` is flagged there, while a
    /// script merely *named* with "watch" but one-shot in body is not.
    #[test]
    fn npm_wrapper_alone_is_not_flagged_by_name() {
        // The bare wrapper carries no watcher signal; name is never sufficient.
        assert!(classify_non_terminating("npm run test", &argv("npm run test")).is_none());
        assert!(
            classify_non_terminating("npm run test:watch", &argv("npm run test:watch")).is_none()
        );
        // The body is what carries the signal (as discover_package_json uses it):
        assert!(classify_non_terminating("vitest --watch", &argv("vitest --watch")).is_some());
        assert!(classify_non_terminating("vitest run", &argv("vitest run")).is_none());
    }
}
