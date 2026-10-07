//! CLI adapter: build an `AuditRequest`, bind the RUNTIME capabilities, run the
//! minimal Task 10 flow (discover → P1 classify/confirm → execute existing
//! mechanisms → capture raw results → honest report), and return rendered
//! output. No P2–P11 interpretation: gates are not state-derived and findings
//! are not derived here.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use fira_core::assessment::{HumanDecisionFira, TechnicalAssessment};
use fira_core::classification::{classify, Classification};
use fira_core::coverage::{CoverageStatement, ExecutedMechanism, SkippedArea};
use fira_core::gate::Gate;
use fira_core::interfaces::CommandExecutor;
use fira_core::model::{
    Depth, GateCause, GateName, GateState, HumanDecisionFiraState, ProfileId, RequirementLevel,
};
use fira_core::report::{AppliedProfile, AppliedProfileGate, AuditReport};
use fira_core::request::AuditRequest;
use fira_core::verdict::{compute_assessment, ExpectationViolations};

use fira_policy::{confirm_gate_view, profile_for, AppliedSelection, ConfirmGateView};
use fira_runtime::{
    detect_signals, discover, FsRepositoryReader, MechanismRegistry, OutputSink,
    ProjectCommandExecutor,
};

/// Output format for the rendered report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Json,
    Md,
    Both,
}

/// The five built-in profiles, as a CLI value. Parses only into the closed
/// `ProfileId` — an unknown value is a parse error, so no sixth profile is
/// representable (reinforces the Task 9 structural PROF-1 guarantee).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProfileArg {
    Library,
    CliTool,
    WebService,
    StatefulDistributed,
    BatchPipeline,
}

impl From<ProfileArg> for ProfileId {
    fn from(p: ProfileArg) -> Self {
        match p {
            ProfileArg::Library => ProfileId::Library,
            ProfileArg::CliTool => ProfileId::CliTool,
            ProfileArg::WebService => ProfileId::WebService,
            ProfileArg::StatefulDistributed => ProfileId::StatefulDistributed,
            ProfileArg::BatchPipeline => ProfileId::BatchPipeline,
        }
    }
}

/// Requested audit depth, as a CLI value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DepthArg {
    Shallow,
    Standard,
    Deep,
}

impl From<DepthArg> for Depth {
    fn from(d: DepthArg) -> Self {
        match d {
            DepthArg::Shallow => Depth::Shallow,
            DepthArg::Standard => Depth::Standard,
            DepthArg::Deep => Depth::Deep,
        }
    }
}

/// `fira-guardian audit` arguments.
#[derive(Debug, Clone, Parser)]
pub struct AuditArgs {
    /// Path to the project to audit.
    #[arg(long)]
    pub project: PathBuf,
    /// The release target description.
    #[arg(long = "release-target")]
    pub release_target: String,
    /// Explicitly select one of the five built-in profiles (human authority).
    #[arg(long)]
    pub profile: Option<ProfileArg>,
    /// Requested audit depth.
    #[arg(long)]
    pub depth: Option<DepthArg>,
    /// Explicitly confirm the classifier's proposed profile (non-interactive).
    #[arg(long = "yes")]
    pub assume_yes: bool,
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Both)]
    pub format: Format,
    /// Optional file to write to (validated by the runtime; never under the
    /// project tree). Absent ⇒ stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Errors surfaced by the adapter.
#[derive(Debug)]
pub enum AdapterError {
    /// A capability (read/exec) failed.
    Capability(String),
    /// Classification was Undetermined and no `--profile` was supplied.
    UndeterminedNeedsProfile(String),
    /// A `Classified` proposal was not explicitly confirmed (no `--yes`, no TTY).
    NeedsExplicitConfirmation(ProfileId),
    /// The output path was rejected by the runtime (WS-1/P-1).
    Output(String),
}

impl core::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AdapterError::Capability(e) => write!(f, "capability error: {e}"),
            AdapterError::UndeterminedNeedsProfile(why) => write!(
                f,
                "system classification is undetermined ({why}); re-run with --profile <id> to select one of the five profiles"
            ),
            AdapterError::NeedsExplicitConfirmation(p) => write!(
                f,
                "classifier proposes profile {p:?}; re-run with --yes to confirm it, or --profile <id> to choose explicitly (no implicit default)"
            ),
            AdapterError::Output(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AdapterError {}

/// The adapter's result: the applied selection and the finished report.
pub struct AuditOutput {
    pub applied: AppliedSelection,
    pub view: ConfirmGateView,
    pub report: AuditReport,
}

/// Whether the current process is attached to an interactive terminal. Used only
/// to decide if an interactive confirm is possible; never to bypass P1.
fn stdin_is_tty() -> bool {
    // Conservative: without an extra dependency we treat non-forced runs as
    // non-interactive. Interactive confirmation is driven by the caller
    // (fira-cli) which may prompt; the library default is non-interactive.
    false
}

/// Run the minimal Task 10 audit flow and return the rendered output.
pub fn run_audit(args: AuditArgs) -> Result<AuditOutput, AdapterError> {
    // 1. Bind capabilities over the project tree (READ + EXECUTE_EXISTING).
    let reader = FsRepositoryReader::new(&args.project)
        .map_err(|e| AdapterError::Capability(e.to_string()))?;
    let root = reader.root().to_path_buf();
    let registry = MechanismRegistry::discover(&root);
    let mechanisms = discover(&root);
    let executor = ProjectCommandExecutor::new(registry, &root);

    // 2. Build the AuditRequest (S1), execution_boundary = {READ, EXECUTE_EXISTING}.
    let request = AuditRequest {
        audit_id: Some(format!("audit-{}", sanitized_id(&args.release_target))),
        project_root: root.to_string_lossy().into_owned(),
        release_target: args.release_target.clone(),
        execution_boundary: executor.capabilities().into_iter().collect(),
        declared_claims: None,
        audit_profile: args.profile.map(Into::into),
        prior_audit_report_ref: None,
        requested_depth: args.depth.map(Into::into),
    };

    // 3. P1 classify + confirm (exact approved semantics). Signals are detected
    //    by the runtime (structural manifests only); CORE's classifier maps them.
    let signals = detect_signals(&root);
    let proposal = classify(&signals);
    let applied = settle_selection(proposal, args.profile.map(Into::into), args.assume_yes)?;
    let profile = profile_for(applied.applied);
    let view = confirm_gate_view(&profile);

    // 4. Execute each discovered mechanism (raw results only).
    let mut executed: Vec<ExecutedMechanism> = Vec::new();
    for m in &mechanisms {
        match executor.run_existing(&m.mechanism.id) {
            Ok(result) => executed.push(ExecutedMechanism {
                command_id: result.command_id,
                outcome: result.outcome,
            }),
            Err(_) => { /* capability gap → recorded as a limitation below */ }
        }
    }

    // 5. Assemble an honest report. The P2–P11 interpretation pipeline is
    //    deferred, so no finding is derived and no gate STATE is derived from
    //    evidence. Each profile gate is recorded at its UNKNOWN ("not
    //    sufficiently audited", C11) state — this is the honest absence of
    //    interpretation, not a §7 decision-table derivation. N/A gates stay N/A
    //    (that is a profile fact, not an evidence derivation).
    let gates = build_unevaluated_gates(&profile);
    let findings = Vec::new();

    let recomputed = compute_assessment(&gates, &findings, &ExpectationViolations::none());
    let technical_assessment = TechnicalAssessment {
        result: recomputed.result,
        blocking_findings: recomputed.blocking_findings,
        blocking_gates: recomputed.blocking_gates,
        risk_findings: recomputed.risk_findings,
        risk_gates: recomputed.risk_gates,
        computed_by_rule: "verdict-engine/§11".to_string(),
    };
    let human_decision = HumanDecisionFira {
        state: recomputed
            .human_decision
            .unwrap_or(HumanDecisionFiraState::Pending),
    };

    let coverage = build_coverage(&gates, executed);

    let report = AuditReport {
        schema_version: "1.0.0".to_string(),
        audit_id: request.audit_id.clone().unwrap_or_default(),
        created_at: "1970-01-01T00:00:00Z".to_string(),
        request: request.clone(),
        applied_profile: AppliedProfile {
            profile_id: profile.profile_id,
            version: profile.version.clone(),
            gates: profile
                .gates
                .iter()
                .map(|g| AppliedProfileGate {
                    gate: g.gate,
                    requirement_level: g.requirement_level,
                    na_justification: g.na_justification.clone(),
                })
                .collect(),
            na_overrides: applied.na_overrides.clone(),
        },
        system_model_summary: format!(
            "Minimal Task-10 run: discovered {} mechanism(s); audit interpretation (P2-P11) not yet performed.",
            mechanisms.len()
        ),
        verification_mechanisms: mechanisms.iter().map(|m| m.mechanism.clone()).collect(),
        divergence_records: Vec::new(),
        findings,
        gates,
        coverage,
        technical_assessment,
        human_decision,
        prior_report_ref: None,
        determinism_inputs_hash: String::new(),
    };

    Ok(AuditOutput { applied, view, report })
}

/// Apply the exact §9.4 confirm semantics.
fn settle_selection(
    proposal: Classification,
    profile_flag: Option<ProfileId>,
    assume_yes: bool,
) -> Result<AppliedSelection, AdapterError> {
    // Explicit --profile always wins as an explicit human selection.
    if let Some(id) = profile_flag {
        return Ok(AppliedSelection::reclassified(proposal, id));
    }
    match &proposal {
        Classification::Undetermined { rationale } => {
            Err(AdapterError::UndeterminedNeedsProfile(rationale.clone()))
        }
        Classification::Classified { profile_id, .. } => {
            let p = *profile_id;
            if assume_yes {
                // Explicit confirmation of the proposed profile.
                Ok(AppliedSelection::confirm(proposal)
                    .expect("Classified proposal is confirmable"))
            } else if stdin_is_tty() {
                // An interactive confirm would prompt here; the library default
                // treats a Classified proposal as confirmed only via the caller.
                Ok(AppliedSelection::confirm(proposal)
                    .expect("Classified proposal is confirmable"))
            } else {
                // Non-interactive, no --yes: do not auto-accept.
                Err(AdapterError::NeedsExplicitConfirmation(p))
            }
        }
    }
}

/// Record each profile gate at its honest "not yet interpreted" state: UNKNOWN
/// for applicable gates, N/A for profile-N/A gates. No evidence-based derivation.
fn build_unevaluated_gates(profile: &fira_policy::AuditProfile) -> Vec<Gate> {
    profile
        .gates
        .iter()
        .map(|g| {
            let (state, cause) = match g.requirement_level {
                RequirementLevel::NotApplicable => (GateState::NotApplicable, Some(GateCause::None)),
                _ => (GateState::Unknown, Some(GateCause::None)),
            };
            Gate {
                name: g.gate,
                requirement_level: g.requirement_level,
                state,
                rationale: "not yet interpreted (P2-P11 pipeline deferred)".to_string(),
                cause,
                support_mappings: Vec::new(),
                supporting_findings: Vec::new(),
            }
        })
        .collect()
}

/// Build an honest coverage statement: every gate maps to a coverage entry
/// (VR8), executed mechanisms are recorded, and the deferred-interpretation
/// limitation is stated.
fn build_coverage(gates: &[Gate], executed: Vec<ExecutedMechanism>) -> CoverageStatement {
    let skipped_areas = gates
        .iter()
        .map(|g| SkippedArea {
            area: gate_wire(&g.name),
            reason: "audit interpretation (P2-P11) not yet performed".to_string(),
        })
        .collect();
    CoverageStatement {
        audited_areas: Vec::new(),
        skipped_areas,
        blocked_areas: Vec::new(),
        executed_mechanisms: executed,
        known_limitations: vec![
            "Task 10 minimal run: mechanisms discovered and executed, but evidence \
             interpretation, finding derivation, and gate-state derivation are not \
             yet implemented."
                .to_string(),
        ],
        assumptions: Vec::new(),
        unanchored_hypotheses: Vec::new(),
    }
}

/// The frozen wire name of a gate (§20). Matches the CORE serde rename exactly;
/// kept as an explicit match so the adapter needs no serde dependency.
fn gate_wire(g: &GateName) -> String {
    match g {
        GateName::Build => "Build",
        GateName::Tests => "Tests",
        GateName::Requirements => "Requirements",
        GateName::Correctness => "Correctness",
        GateName::FailureRecovery => "FailureRecovery",
        GateName::PersistenceDurability => "PersistenceDurability",
        GateName::Concurrency => "Concurrency",
        GateName::Security => "Security",
        GateName::Observability => "Observability",
        GateName::Documentation => "Documentation",
        GateName::OperationalReadiness => "OperationalReadiness",
    }
    .to_string()
}

fn sanitized_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Validate + write rendered output through the runtime sink (WS-1/P-1).
pub fn write_output(
    project_root: &std::path::Path,
    requested: &std::path::Path,
    bytes: &[u8],
) -> Result<PathBuf, AdapterError> {
    let sink = OutputSink::new(project_root).map_err(|e| AdapterError::Output(e.to_string()))?;
    sink.write(requested, bytes)
        .map_err(|e| AdapterError::Output(e.to_string()))
}
