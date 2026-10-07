//! VerificationMechanism registry + `CommandExecutor` (EXEC-1, §6/§17).
//!
//! The executor runs **only** mechanisms discovered in the project
//! (`declared_by_project=true`), looked up by `command_id`. An id not in the
//! registry is rejected (`CapabilityError::NotFound`) — the runtime enforcement
//! of EXEC-1. There is no arbitrary-shell API: execution uses
//! `std::process::Command` with the discovered **explicit argument vector**, no
//! shell interpolation, and no network.
//!
//! Task 10 produces a **raw** `ExecutionResult` (outcome + captured output). The
//! §6 outcome→epistemic/gate interpretation is deliberately **not** done here
//! (deferred to later CORE orchestration).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use fira_core::execution::{ExecutionResult, VerificationMechanism};
use fira_core::interfaces::{CapabilityError, CommandExecutor};
use fira_core::model::{Capability, ExecutionOutcome, IdRef};

use crate::discovery::{discover, DiscoveredMechanism};

/// The discovered registry: project-declared mechanisms + their argv. The only
/// commands the executor will ever run.
#[derive(Debug, Clone, Default)]
pub struct MechanismRegistry {
    entries: Vec<DiscoveredMechanism>,
}

impl MechanismRegistry {
    /// Discover mechanisms under `project_root`.
    pub fn discover(project_root: &std::path::Path) -> Self {
        MechanismRegistry {
            entries: discover(project_root),
        }
    }

    /// Build from an explicit list (used in tests).
    pub fn from_entries(entries: Vec<DiscoveredMechanism>) -> Self {
        MechanismRegistry { entries }
    }

    /// The S12 mechanism records (never synthesized).
    pub fn mechanisms(&self) -> Vec<VerificationMechanism> {
        self.entries.iter().map(|e| e.mechanism.clone()).collect()
    }

    fn find(&self, command_id: &IdRef) -> Option<&DiscoveredMechanism> {
        self.entries.iter().find(|e| &e.mechanism.id == command_id)
    }
}

/// Executes existing, project-declared mechanisms (EXECUTE_EXISTING; EXEC-1).
#[derive(Debug, Clone)]
pub struct ProjectCommandExecutor {
    registry: MechanismRegistry,
    cwd: PathBuf,
}

impl ProjectCommandExecutor {
    /// Build an executor whose working directory is the project root.
    pub fn new(registry: MechanismRegistry, project_root: impl Into<PathBuf>) -> Self {
        ProjectCommandExecutor {
            registry,
            cwd: project_root.into(),
        }
    }
}

impl CommandExecutor for ProjectCommandExecutor {
    fn run_existing(&self, command_id: &IdRef) -> Result<ExecutionResult, CapabilityError> {
        // EXEC-1: reject any id not in the discovered registry.
        let entry = self
            .registry
            .find(command_id)
            .ok_or_else(|| CapabilityError::NotFound(command_id.0.clone()))?;

        // Explicit argv; no shell. argv[0] is the program.
        let (program, args) = entry
            .argv
            .split_first()
            .ok_or_else(|| CapabilityError::Failed("empty argument vector".to_string()))?;

        let mut cmd = Command::new(program);
        cmd.args(args).current_dir(&self.cwd);
        // No env for network; inherit none of the shell. (No shell is spawned.)

        let output = cmd
            .output()
            .map_err(|e| CapabilityError::Failed(format!("spawn failed: {e}")))?;

        let mut captured = String::new();
        captured.push_str(&String::from_utf8_lossy(&output.stdout));
        if !output.stderr.is_empty() {
            captured.push_str(&String::from_utf8_lossy(&output.stderr));
        }

        // Raw outcome only: success/failure of the process. No §6 interpretation,
        // no classification inference, no exercised-behavior inference.
        let outcome = if output.status.success() {
            ExecutionOutcome::Passed
        } else {
            ExecutionOutcome::Failed
        };

        Ok(ExecutionResult {
            command_id: entry.mechanism.id.clone(),
            command: entry.mechanism.command.clone(),
            outcome,
            classification: None,
            blocking_condition: None,
            captured_output: captured,
            exercised_behaviors: Vec::new(),
        })
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        // MVP boundary: READ + EXECUTE_EXISTING. NETWORK is never included.
        let mut caps = BTreeSet::new();
        caps.insert(Capability::Read);
        caps.insert(Capability::ExecuteExisting);
        caps
    }

    fn discover_mechanisms(&self) -> Result<Vec<VerificationMechanism>, CapabilityError> {
        Ok(self.registry.mechanisms())
    }
}
