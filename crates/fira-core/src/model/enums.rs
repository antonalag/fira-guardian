//! Shared closed enums for the CORE domain model.
//!
//! Each enum's wire form (via `serde` rename) equals the schema enum string
//! verbatim. The enums are intentionally **not** `#[non_exhaustive]`: an
//! exhaustive `match` over the frozen variant set is a deliberate property
//! (illegal states unrepresentable — see `docs/adr/ADR-001`).

use serde::{Deserialize, Serialize};

/// EpistemicState facets (S3; frozen contract §3). Orthogonal, not a ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Facet {
    #[serde(rename = "SPECIFIED")]
    Specified,
    #[serde(rename = "IMPLEMENTED")]
    Implemented,
    #[serde(rename = "TESTED")]
    Tested,
    #[serde(rename = "OBSERVED")]
    Observed,
}

/// EpistemicState conclusion (S3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EpistemicConclusion {
    #[serde(rename = "VERIFIED")]
    Verified,
    #[serde(rename = "UNVERIFIED")]
    Unverified,
}

/// MVP project-tree capabilities (CAP-1). MVP = {READ, EXECUTE_EXISTING}.
///
/// `Ord` is derived so the §17 `CommandExecutor::capabilities() ->
/// BTreeSet<Capability>` return type is usable; it adds no variant and no wire
/// change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Capability {
    #[serde(rename = "READ")]
    Read,
    #[serde(rename = "EXECUTE_EXISTING")]
    ExecuteExisting,
}

/// Requested audit depth (S1, S9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Depth {
    #[serde(rename = "SHALLOW")]
    Shallow,
    #[serde(rename = "STANDARD")]
    Standard,
    #[serde(rename = "DEEP")]
    Deep,
}

/// Profile requirement level for a gate (S2, S8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequirementLevel {
    #[serde(rename = "required")]
    Required,
    #[serde(rename = "recommended")]
    Recommended,
    #[serde(rename = "not_applicable")]
    NotApplicable,
}

/// Finding maturity (S7). OBSERVATION/HYPOTHESIS live in the coverage appendix;
/// the publishable maturities are FINDING and RISK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Maturity {
    #[serde(rename = "FINDING")]
    Finding,
    #[serde(rename = "RISK")]
    Risk,
}

/// Finding severity (S7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Severity {
    #[serde(rename = "CRITICAL")]
    Critical,
    #[serde(rename = "HIGH")]
    High,
    #[serde(rename = "MEDIUM")]
    Medium,
    #[serde(rename = "LOW")]
    Low,
    #[serde(rename = "INFO")]
    Info,
}

/// Derived confidence (S7; CONF-1). LOW != INSUFFICIENT (CONF-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Confidence {
    #[serde(rename = "HIGH")]
    High,
    #[serde(rename = "MEDIUM")]
    Medium,
    #[serde(rename = "LOW")]
    Low,
}

/// Finding requirement source (S7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequirementSource {
    #[serde(rename = "declared_claim")]
    DeclaredClaim,
    #[serde(rename = "inferred_invariant")]
    InferredInvariant,
    #[serde(rename = "release_requirement")]
    ReleaseRequirement,
}

/// SecurityProperty source (S6, S7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SecurityPropertySource {
    #[serde(rename = "declared")]
    Declared,
    #[serde(rename = "inferred")]
    Inferred,
    #[serde(rename = "profile")]
    Profile,
}

/// Finding release impact (S7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReleaseImpact {
    #[serde(rename = "BLOCKER")]
    Blocker,
    #[serde(rename = "RISK")]
    Risk,
    #[serde(rename = "NON_BLOCKING")]
    NonBlocking,
}

/// Finding lifecycle status (S7; frozen contract §14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LifecycleStatus {
    #[serde(rename = "OPEN")]
    Open,
    #[serde(rename = "FIX_CLAIMED")]
    FixClaimed,
    #[serde(rename = "VERIFIED_FIXED")]
    VerifiedFixed,
    #[serde(rename = "REOPENED")]
    Reopened,
    #[serde(rename = "WONT_FIX")]
    WontFix,
}

/// Gate state (S8; frozen contract §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GateState {
    #[serde(rename = "PASS")]
    Pass,
    #[serde(rename = "FAIL")]
    Fail,
    #[serde(rename = "PARTIAL")]
    Partial,
    #[serde(rename = "UNKNOWN")]
    Unknown,
    #[serde(rename = "N/A")]
    NotApplicable,
}

/// Gate cause annotation (S8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GateCause {
    #[serde(rename = "APP")]
    App,
    #[serde(rename = "INFRA")]
    Infra,
    #[serde(rename = "NONE")]
    None,
}

/// Coverage entry result (S9). NO_ISSUE_FOUND never implies proof of correctness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CoverageResult {
    #[serde(rename = "NO_ISSUE_FOUND")]
    NoIssueFound,
    #[serde(rename = "ISSUES_FOUND")]
    IssuesFound,
}

/// Evidence reference type (S4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvidenceType {
    #[serde(rename = "CODE")]
    Code,
    #[serde(rename = "TEST")]
    Test,
    #[serde(rename = "CONFIG")]
    Config,
    #[serde(rename = "COMMAND")]
    Command,
    #[serde(rename = "DOC")]
    Doc,
}

/// SupportMapping sufficiency (S4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sufficiency {
    #[serde(rename = "SUFFICIENT")]
    Sufficient,
    #[serde(rename = "INSUFFICIENT")]
    Insufficient,
}

/// ExecutionResult outcome (S5; frozen contract §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionOutcome {
    #[serde(rename = "PASSED")]
    Passed,
    #[serde(rename = "FAILED")]
    Failed,
    #[serde(rename = "NOT_RUN")]
    NotRun,
    #[serde(rename = "BLOCKED")]
    Blocked,
    #[serde(rename = "TIMEOUT")]
    Timeout,
    #[serde(rename = "ERROR")]
    Error,
}

/// ExecutionResult classification (S5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionClassification {
    #[serde(rename = "APP_LEVEL")]
    AppLevel,
    #[serde(rename = "INFRA_LEVEL")]
    InfraLevel,
    #[serde(rename = "UNCLASSIFIED")]
    Unclassified,
}

/// VerificationMechanism source (S12; frozen contract §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerificationMechanismSource {
    #[serde(rename = "MAVEN")]
    Maven,
    #[serde(rename = "GRADLE")]
    Gradle,
    #[serde(rename = "NPM")]
    Npm,
    #[serde(rename = "MAKE")]
    Make,
    #[serde(rename = "CI")]
    Ci,
    #[serde(rename = "DOCKER")]
    Docker,
    #[serde(rename = "SCRIPT")]
    Script,
    #[serde(rename = "OTHER")]
    Other,
}

/// TechnicalAssessment result (S10; frozen contract §10). FIRA-owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TechnicalAssessmentResult {
    #[serde(rename = "READY")]
    Ready,
    #[serde(rename = "READY_WITH_RISKS")]
    ReadyWithRisks,
    #[serde(rename = "NOT_READY")]
    NotReady,
}

/// Full external HumanDecision state (S10). FIRA never emits ACCEPTED/REJECTED;
/// the FIRA-produced path uses [`HumanDecisionFiraState`] (VR6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HumanDecisionState {
    #[serde(rename = "NOT_APPLICABLE")]
    NotApplicable,
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "ACCEPTED")]
    Accepted,
    #[serde(rename = "REJECTED")]
    Rejected,
}

/// FIRA-produced HumanDecision state (VR6): restricted to {NOT_APPLICABLE,
/// PENDING}. The restriction makes a FIRA-emitted ACCEPTED/REJECTED
/// unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HumanDecisionFiraState {
    #[serde(rename = "NOT_APPLICABLE")]
    NotApplicable,
    #[serde(rename = "PENDING")]
    Pending,
}

/// DivergenceRecord divergence type (S11; frozen contract §12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DivergenceType {
    #[serde(rename = "DOC_OVERCLAIMS")]
    DocOverclaims,
    #[serde(rename = "DOC_UNDERCLAIMS")]
    DocUnderclaims,
    #[serde(rename = "DOC_CONTRADICTS_IMPL")]
    DocContradictsImpl,
    #[serde(rename = "DOC_MATCHES")]
    DocMatches,
}

/// The 11 gates (frozen contract §20). A closed enum so a gate name outside the
/// frozen set is unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GateName {
    Build,
    Tests,
    Requirements,
    Correctness,
    FailureRecovery,
    PersistenceDurability,
    Concurrency,
    Security,
    Observability,
    Documentation,
    OperationalReadiness,
}

/// The 5 built-in profile ids (§20, S2). The id enum lives in CORE (S1/S11
/// reference it); the profile *data* (S2 instances) is POLICY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProfileId {
    #[serde(rename = "library")]
    Library,
    #[serde(rename = "cli_tool")]
    CliTool,
    #[serde(rename = "web_service")]
    WebService,
    #[serde(rename = "stateful_distributed")]
    StatefulDistributed,
    #[serde(rename = "batch_pipeline")]
    BatchPipeline,
}
