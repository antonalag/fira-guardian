//! RUNTIME execution tests: discovery of project-declared mechanisms and EXEC-1
//! enforcement (`run_existing` rejects anything not in the discovered registry).

use std::fs;

use fira_core::interfaces::{CapabilityError, CommandExecutor};
use fira_core::model::{Capability, ExecutionOutcome, IdRef};
use fira_runtime::{discover, MechanismRegistry, ProjectCommandExecutor};
use tempfile::tempdir;

/// Discovery records project-declared Cargo mechanisms with declared_by_project.
#[test]
fn discovers_cargo_mechanisms() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let found = discover(dir.path());
    let ids: Vec<&str> = found.iter().map(|m| m.mechanism.id.0.as_str()).collect();
    assert!(ids.contains(&"cargo-build"));
    assert!(ids.contains(&"cargo-test"));
    // All discovered mechanisms are project-declared (never synthesized): the
    // AlwaysTrue type guarantees declared_by_project == true structurally.
    assert!(!found.is_empty());
}

/// Discovery records project-declared Makefile targets but ignores recipes and
/// variable assignments.
#[test]
fn discovers_makefile_targets() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Makefile"),
        "VAR = 1\nbuild: deps\n\techo building\ntest:\n\techo testing\n",
    )
    .unwrap();

    let found = discover(dir.path());
    let ids: Vec<&str> = found.iter().map(|m| m.mechanism.id.0.as_str()).collect();
    assert!(ids.contains(&"make-build"));
    assert!(ids.contains(&"make-test"));
    assert!(!ids.contains(&"make-VAR")); // assignment ignored
}

/// Colon-adjacent variable assignments (`:=`, `::=`, `:::=`) are not targets:
/// the first colon belongs to the assignment operator, so these lines must not
/// be emitted as executable mechanisms. Genuine `name:` / `name: deps` targets
/// on neighboring lines remain discoverable.
#[test]
fn makefile_variable_assignments_are_not_targets() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Makefile"),
        concat!(
            "COMPOSE_FILE := docker-compose.yml\n",
            "BOOTSTRAP ::= scripts/bootstrap.sh\n",
            "E2E_COMPOSE :::= compose.e2e.yml\n",
            "SEED_SCRIPT := scripts/seed.sh\n",
            "build: deps\n",
            "\techo building\n",
            "test:\n",
            "\techo testing\n",
        ),
    )
    .unwrap();

    let found = discover(dir.path());
    let ids: Vec<&str> = found.iter().map(|m| m.mechanism.id.0.as_str()).collect();

    // Genuine targets survive.
    assert!(ids.contains(&"make-build"));
    assert!(ids.contains(&"make-test"));

    // Phantom mechanisms from `:=` / `::=` / `:::=` assignments are absent.
    assert!(!ids.contains(&"make-COMPOSE_FILE"));
    assert!(!ids.contains(&"make-BOOTSTRAP"));
    assert!(!ids.contains(&"make-E2E_COMPOSE"));
    assert!(!ids.contains(&"make-SEED_SCRIPT"));
}

/// EXEC-1: `run_existing` rejects an id not in the discovered registry
/// (a synthesized command is refused).
#[test]
fn run_existing_rejects_unknown_command_id() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let registry = MechanismRegistry::discover(dir.path());
    let executor = ProjectCommandExecutor::new(registry, dir.path());

    let err = executor
        .run_existing(&IdRef("synth-rm-rf".to_string()))
        .unwrap_err();
    assert_eq!(err, CapabilityError::NotFound("synth-rm-rf".to_string()));
}

/// EXEC-1: a discovered mechanism runs and yields a raw ExecutionResult (no §6
/// interpretation: classification stays None, exercised_behaviors empty).
#[test]
fn run_existing_runs_discovered_mechanism() {
    let dir = tempdir().unwrap();
    // A Makefile target that runs a trivial, always-available command.
    fs::write(dir.path().join("Makefile"), "noop:\n\ttrue\n").unwrap();
    let registry = MechanismRegistry::discover(dir.path());
    let executor = ProjectCommandExecutor::new(registry, dir.path());

    // `make` may not exist in every environment; only assert on the result shape
    // if it ran, and on EXEC-1 acceptance (the id resolves) regardless.
    match executor.run_existing(&IdRef("make-noop".to_string())) {
        Ok(result) => {
            assert_eq!(result.command_id, IdRef("make-noop".to_string()));
            assert!(result.classification.is_none(), "no §6 interpretation in Task 10");
            assert!(result.exercised_behaviors.is_empty());
            assert!(matches!(
                result.outcome,
                ExecutionOutcome::Passed | ExecutionOutcome::Failed
            ));
        }
        Err(CapabilityError::Failed(_)) => {
            // `make` not installed in this environment: the id still resolved
            // (EXEC-1 passed); only the spawn failed. Acceptable for this test.
        }
        Err(other) => panic!("unexpected error: {other:?}"),
    }
}

/// The executor advertises only {READ, EXECUTE_EXISTING}; NETWORK is never present.
#[test]
fn executor_capabilities_are_bounded() {
    let dir = tempdir().unwrap();
    let executor = ProjectCommandExecutor::new(MechanismRegistry::default(), dir.path());
    let caps = executor.capabilities();
    assert!(caps.contains(&Capability::Read));
    assert!(caps.contains(&Capability::ExecuteExisting));
    assert_eq!(caps.len(), 2);
}
