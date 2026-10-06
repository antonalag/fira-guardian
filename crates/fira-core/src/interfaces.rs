//! Abstract capability interfaces (§17): the four traits CORE calls. Signatures
//! only — the implementations (enforcement, process execution, discovery,
//! persistence) are RUNTIME/adapter concerns.
//!
//! The traits are object-safe so CORE can hold them as `dyn` capability handles.
//! Fallible operations return [`Result<_, CapabilityError>`] rather than
//! panicking, so a capability gap becomes a coverage limitation, not a crash.

use std::collections::BTreeSet;

use crate::evidence::{EvidenceRef, SupportMapping};
use crate::execution::{ExecutionResult, VerificationMechanism};
use crate::finding::Finding;
use crate::gate::Gate;
use crate::model::{Capability, Depth, IdRef};
use crate::report::AuditReport;
use crate::request::AuditRequest;

/// Error returned when a capability is unavailable or an operation cannot be
/// completed. A CORE-pure, host-agnostic data enum — it is **not** an I/O error
/// type. Its presence lets CORE turn a capability gap into a coverage limitation
/// instead of a crash (§17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityError {
    /// The capability is not provided by the current host binding.
    Unavailable,
    /// The capability exists but the request is outside the permitted set
    /// (e.g. a project-tree write; CAP-1 denies it).
    NotPermitted,
    /// The requested target could not be resolved (e.g. an unknown locator or
    /// `command_id`). Carries a human-readable locator for diagnostics.
    NotFound(String),
    /// The capability is present but failed to produce a result; the string is a
    /// diagnostic message (no host error type is referenced from CORE).
    Failed(String),
}

impl core::fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CapabilityError::Unavailable => write!(f, "capability unavailable"),
            CapabilityError::NotPermitted => write!(f, "operation not permitted by capability set"),
            CapabilityError::NotFound(what) => write!(f, "not found: {what}"),
            CapabilityError::Failed(why) => write!(f, "capability failed: {why}"),
        }
    }
}

/// An inclusive line range within a file, for scoped reads. CORE-pure; carries no
/// filesystem semantics (resolution against a real file is a runtime concern).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

/// The location where a report was persisted (returned by
/// [`AuditContextProvider::persist_report`]). A result-location value only; CORE
/// neither writes nor validates it, and WS-1 path enforcement lives in RUNTIME.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistLocation(pub String);

/// Reads the audited project tree (READ capability). §17.
pub trait RepositoryReader {
    /// Read a file, optionally scoped to a line range.
    fn read_file(&self, path: &str, range: Option<LineRange>) -> Result<String, CapabilityError>;

    /// List structure under a path, optionally bounded by depth.
    fn list_structure(&self, path: &str, depth: Option<Depth>)
        -> Result<Vec<String>, CapabilityError>;

    /// Resolve whether a locator points at something that exists.
    fn resolve_ref(&self, locator: &str) -> Result<bool, CapabilityError>;
}

/// Executes existing, project-declared verification mechanisms
/// (EXECUTE_EXISTING capability; EXEC-1). §17.
pub trait CommandExecutor {
    /// Run an existing mechanism by id. Never synthesizes a command (EXEC-1); the
    /// id must resolve to a discovered `declared_by_project=true` mechanism — a
    /// guarantee the runtime enforces, not this abstract signature.
    fn run_existing(&self, command_id: &IdRef) -> Result<ExecutionResult, CapabilityError>;

    /// The capability set this executor provides (MVP ⊆ {READ, EXECUTE_EXISTING}).
    fn capabilities(&self) -> BTreeSet<Capability>;

    /// The discovered verification-mechanism registry (never synthesized).
    fn discover_mechanisms(&self) -> Result<Vec<VerificationMechanism>, CapabilityError>;
}

/// Captures execution output as evidence and attaches support mappings. §17.
///
/// The §17 `attach(finding|gate, support_mapping)` operation is split into two
/// methods ([`attach_to_finding`](EvidenceCollector::attach_to_finding) /
/// [`attach_to_gate`](EvidenceCollector::attach_to_gate)); the split is a
/// representation choice and does not change contract semantics.
pub trait EvidenceCollector {
    /// Capture an execution result as an evidence reference.
    fn capture(&self, execution_result: &ExecutionResult) -> Result<EvidenceRef, CapabilityError>;

    /// Attach a support mapping to a finding.
    fn attach_to_finding(
        &self,
        finding: &mut Finding,
        mapping: SupportMapping,
    ) -> Result<(), CapabilityError>;

    /// Attach a support mapping to a gate.
    fn attach_to_gate(
        &self,
        gate: &mut Gate,
        mapping: SupportMapping,
    ) -> Result<(), CapabilityError>;
}

/// Provides audit context and persists the report. §17.
pub trait AuditContextProvider {
    /// The current audit request (TrustedInputs as scope/context, not truth).
    fn get_request(&self) -> Result<AuditRequest, CapabilityError>;

    /// A prior report, if any (historical evidence, never current truth — C12).
    fn get_prior_report(&self) -> Result<Option<AuditReport>, CapabilityError>;

    /// Persist the report and return its location. Persistence targets the
    /// external audit workspace (WS-1) and never the project tree; that
    /// enforcement is a runtime/adapter concern — CORE only names the result.
    fn persist_report(&self, report: &AuditReport) -> Result<PersistLocation, CapabilityError>;
}
