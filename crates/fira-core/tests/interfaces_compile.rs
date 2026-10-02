//! AC-5: the four §17 capability interfaces are declarable, object-safe, and
//! usable via `dyn` — proven by a trivial **test-target** implementation that
//! performs no I/O.
//!
//! This implementation lives in the test target, not in `fira-core` src, so CORE
//! ships only the abstract traits (no implementation, no host behavior). Every
//! method here returns a `CapabilityError` or an empty/placeholder value — it
//! touches no filesystem, process, network, or persistence.

use std::collections::BTreeSet;

use fira_core::evidence::{EvidenceRef, SupportMapping};
use fira_core::execution::{ExecutionResult, VerificationMechanism};
use fira_core::finding::Finding;
use fira_core::gate::Gate;
use fira_core::interfaces::{
    AuditContextProvider, CapabilityError, CommandExecutor, EvidenceCollector, LineRange,
    PersistLocation, RepositoryReader,
};
use fira_core::model::{Capability, Depth, IdRef};
use fira_core::report::AuditReport;
use fira_core::request::AuditRequest;

/// A no-capability stub: every operation reports the capability as unavailable.
/// No I/O of any kind.
struct NullCapabilities;

impl RepositoryReader for NullCapabilities {
    fn read_file(&self, _path: &str, _range: Option<LineRange>) -> Result<String, CapabilityError> {
        Err(CapabilityError::Unavailable)
    }
    fn list_structure(
        &self,
        _path: &str,
        _depth: Option<Depth>,
    ) -> Result<Vec<String>, CapabilityError> {
        Err(CapabilityError::Unavailable)
    }
    fn resolve_ref(&self, _locator: &str) -> Result<bool, CapabilityError> {
        Err(CapabilityError::Unavailable)
    }
}

impl CommandExecutor for NullCapabilities {
    fn run_existing(&self, _command_id: &IdRef) -> Result<ExecutionResult, CapabilityError> {
        Err(CapabilityError::Unavailable)
    }
    fn capabilities(&self) -> BTreeSet<Capability> {
        BTreeSet::new()
    }
    fn discover_mechanisms(&self) -> Result<Vec<VerificationMechanism>, CapabilityError> {
        Err(CapabilityError::Unavailable)
    }
}

impl EvidenceCollector for NullCapabilities {
    fn capture(&self, _execution_result: &ExecutionResult) -> Result<EvidenceRef, CapabilityError> {
        Err(CapabilityError::Unavailable)
    }
    fn attach_to_finding(
        &self,
        _finding: &mut Finding,
        _mapping: SupportMapping,
    ) -> Result<(), CapabilityError> {
        Err(CapabilityError::Unavailable)
    }
    fn attach_to_gate(
        &self,
        _gate: &mut Gate,
        _mapping: SupportMapping,
    ) -> Result<(), CapabilityError> {
        Err(CapabilityError::Unavailable)
    }
}

impl AuditContextProvider for NullCapabilities {
    fn get_request(&self) -> Result<AuditRequest, CapabilityError> {
        Err(CapabilityError::Unavailable)
    }
    fn get_prior_report(&self) -> Result<Option<AuditReport>, CapabilityError> {
        Ok(None)
    }
    fn persist_report(&self, _report: &AuditReport) -> Result<PersistLocation, CapabilityError> {
        Err(CapabilityError::Unavailable)
    }
}

/// The traits are object-safe: hold them behind `dyn` references.
#[test]
fn capability_traits_are_object_safe() {
    let stub = NullCapabilities;
    let reader: &dyn RepositoryReader = &stub;
    let executor: &dyn CommandExecutor = &stub;
    let collector: &dyn EvidenceCollector = &stub;
    let context: &dyn AuditContextProvider = &stub;

    assert!(matches!(
        reader.resolve_ref("x"),
        Err(CapabilityError::Unavailable)
    ));
    assert!(executor.capabilities().is_empty());
    assert!(matches!(
        collector.capture(&sample_execution_result()),
        Err(CapabilityError::Unavailable)
    ));
    assert!(matches!(context.get_prior_report(), Ok(None)));
}

/// The CORE-pure error enum is usable and `Display`-able without any host type.
#[test]
fn capability_error_is_pure_data() {
    let e = CapabilityError::NotFound("cmd-x".to_string());
    assert_eq!(e.to_string(), "not found: cmd-x");
    assert_eq!(CapabilityError::Unavailable, CapabilityError::Unavailable);
}

/// `PersistLocation` represents the location concept only; CORE never writes.
#[test]
fn persist_location_is_a_plain_newtype() {
    let loc = PersistLocation("/fira-workspace/audit-1/report.json".to_string());
    assert_eq!(loc.0, "/fira-workspace/audit-1/report.json");
}

fn sample_execution_result() -> ExecutionResult {
    ExecutionResult {
        command_id: IdRef("cmd-1".to_string()),
        command: "make test".to_string(),
        outcome: fira_core::model::ExecutionOutcome::Passed,
        classification: None,
        blocking_condition: None,
        captured_output: String::new(),
        exercised_behaviors: Vec::new(),
    }
}
