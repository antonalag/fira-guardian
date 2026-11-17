//! CLI adapter: build an `AuditRequest`, bind the RUNTIME capabilities, run the
//! minimal Task 10 flow (discover → P1 classify/confirm → execute existing
//! mechanisms → capture raw results → honest report), and return rendered
//! output. No P2–P11 interpretation: gates are not state-derived and findings
//! are not derived here.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use fira_core::assessment::{HumanDecisionFira, TechnicalAssessment};
use fira_core::classification::{classify, Classification};
use fira_core::coverage::{
    AuditedArea, BlockedArea, CoverageStatement, ExecutedMechanism, SkippedArea,
};
use fira_core::execution::{ExecutionResult, VerificationMechanism};
use fira_core::gate::Gate;
use fira_core::gate_eval::{
    backing_gate, evaluate_execution_backed_gates_with_unexecuted, ExecutedMechanismResult,
};
use fira_core::interfaces::{CommandExecutor, RepositoryReader};
use fira_core::model::{CoverageResult, ExecutionClassification, ExecutionOutcome};
use fira_core::model::{
    Depth, GateCause, GateName, GateState, HumanDecisionFiraState, ProfileId, RequirementLevel,
};
use fira_core::property_audit::{
    classify_fencing, FencingAssessment, FencingEvidence, PropertyExercised, FENCING_GATE,
};
use fira_core::provenance::{assess_with_inputs_hash, format_utc_timestamp};
use fira_core::report::{AppliedProfile, AppliedProfileGate, AuditReport};
use fira_core::request::AuditRequest;

use fira_core::confidence::ConfidenceRubric;
use fira_core::validation::{run_vrs, RecomputeHook, StaticValidationContext, Violation};
use fira_core::verdict::VerdictEngine;

use fira_core::interfaces::AuditContextProvider;

use fira_policy::{confirm_gate_view, profile_for, AppliedSelection, ConfirmGateView};
use fira_runtime::{
    detect_signals, discover, DiscoveredMechanism, FsRepositoryReader, MechanismRegistry,
    OutputSink, ProjectCommandExecutor,
};

use crate::persistence::{resolve_default_base, AdapterAuditContextProvider};

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
    /// Optional file to write an *exported copy* to (validated by the runtime;
    /// never under the project tree). Absent ⇒ stdout. This is distinct from the
    /// canonical audit-workspace artifacts (§9.4).
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Audit-workspace base directory (the canonical persistence location, WS-1).
    /// Absent ⇒ a per-user platform data dir is resolved (`std::env` only, §4.1);
    /// if none qualifies the run fails asking for `--workspace`. Never the
    /// project tree.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
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
    /// The audit-workspace base could not be resolved (no `--workspace` and no
    /// qualifying default location).
    WorkspaceUnresolved(String),
    /// Persisting the canonical report to the audit workspace failed (WS-1).
    Persist(String),
    /// The report's provenance (`created_at` / `determinism_inputs_hash`)
    /// could not be established; no fabricated value is emitted.
    Provenance(String),
    /// The assembled report was persisted (diagnostic evidence preserved) but
    /// failed one or more VR validation checks (`run_vrs` / VR5 / VR7). Carries
    /// the canonical persisted location and every collected violation, in the
    /// order they were produced (no cap, no truncation — D-12).
    VrViolations {
        report_location: String,
        violations: Vec<Violation>,
    },
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
            AdapterError::WorkspaceUnresolved(e) => write!(f, "{e}"),
            AdapterError::Persist(e) => write!(f, "failed to persist report to audit workspace: {e}"),
            AdapterError::Provenance(e) => write!(f, "failed to establish report provenance: {e}"),
            AdapterError::VrViolations {
                report_location,
                violations,
            } => {
                write!(
                    f,
                    "VR validation failed for the persisted report at {report_location}: {} violation(s)",
                    violations.len()
                )?;
                for v in violations {
                    // Reuse `Violation`'s own Display (VR id, locator, detail);
                    // every violation is listed (D-12), not just the first.
                    write!(f, "\n  - {v}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for AdapterError {}

/// The adapter's result: the applied selection, the finished report, and the
/// canonical audit-workspace location it was persisted to (§9.4).
pub struct AuditOutput {
    pub applied: AppliedSelection,
    pub view: ConfirmGateView,
    pub report: AuditReport,
    /// The canonical `<base>/<audit-id>/` directory where `report.{json,md}` were
    /// written (WS-1). This is authoritative; stdout/`--output` are exported
    /// copies.
    pub canonical_location: String,
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
    // One discovery: the executor registry and the iterated collection are the
    // same (R3.6), so there is no second, independent read of the filesystem.
    let mechanisms = discover(&root);
    let executor = executor_for(&mechanisms, &root);

    // 2. Build the AuditRequest (S1), execution_boundary = {READ, EXECUTE_EXISTING}.
    // Compose a distinct id per run so a fresh audit never collides with another
    // by construction (design §2.4: `<sanitized-release-target>-<nonce>`). The
    // nonce is a `std::time`-derived token — no new crate, no network.
    let audit_id = format!(
        "audit-{}-{}",
        sanitized_id(&args.release_target),
        audit_nonce()
    );
    let request = AuditRequest {
        audit_id: Some(audit_id),
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
    //    The raw results, the rows, the limitations and the backing mechanisms
    //    without a result are all collected by `run_mechanisms`.
    let phase = run_mechanisms(&executor, &mechanisms);

    // 5. Assemble an honest report. The P2–P11 interpretation pipeline is
    //    deferred, so no finding is derived and no gate STATE is derived from
    //    evidence — EXCEPT the execution-grounded Tests/Build slice. Every gate
    //    starts UNKNOWN ("not sufficiently audited", C11; N/A stays a profile
    //    fact); then the pure CORE `gate_eval` step derives the Tests and Build
    //    gate states from the real ExecutionResults (§6/§7). Only those two gates
    //    may change; findings remain empty (no P2-P8 synthesis in this slice).
    let mut gates = build_unevaluated_gates(&profile);
    let profile_gate_levels: Vec<(GateName, RequirementLevel)> = profile
        .gates
        .iter()
        .map(|g| (g.gate, g.requirement_level))
        .collect();
    let evaluated = gates_from_phase(&profile_gate_levels, &phase);
    // Merge: a gate_eval result replaces the UNKNOWN default for the same gate.
    for ev in evaluated {
        if let Some(slot) = gates.iter_mut().find(|g| g.name == ev.name) {
            *slot = ev;
        }
    }

    // Single-property semantic audit (one targeted property: the JournalStore
    // recovery-generation fencing invariant). P2 reads the targeted source via
    // the READ capability; CORE classifies the evidence on the §3 facet ladder.
    // Under the approved Option A, FIRA cannot independently demonstrate that the
    // specific fencing behavior was *exercised* by the executed test mechanism,
    // so the honest ceiling is UNVERIFIED even when the suite PASSED — a passing
    // suite is not proof of this property (C5/C6/VR11). The FailureRecovery gate
    // therefore stays UNKNOWN (never PASS from this property), and the property's
    // evidence state is recorded as a coverage limitation (C11). No finding is
    // published (an UNVERIFIED property has no SUFFICIENT execution evidence —
    // CONF-2), so `findings` stays empty.
    let fencing = audit_fencing_property(&reader, npm_test_passed(&phase.executed_results));
    if let Some(assessment) = &fencing {
        if let Some(slot) = gates.iter_mut().find(|g| g.name == FENCING_GATE) {
            // Record the property's evidence state on the gate's rationale; the
            // state remains UNKNOWN (UNVERIFIED ⇒ never PASS, C5).
            slot.rationale = format!("fencing invariant audited: {}", assessment.rationale);
        }
    }

    let findings = Vec::new();

    // Single CORE entry point: the same gates, findings and (empty) expectation
    // set feed both the unchanged verdict engine and `determinism_inputs_hash`
    // (report-provenance-slice R2.3). `&[]` ≡ `ExpectationViolations::none()`.
    let (recomputed, determinism_inputs_hash) = assess_with_inputs_hash(&gates, &findings, &[])
        .map_err(|e| AdapterError::Provenance(e.to_string()))?;
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

    let blocked_areas = infra_blocked_areas(&phase.executed_results, &gates);
    let ExecutionPhase {
        executed,
        limitations: mechanism_limitations,
        ..
    } = phase;
    let mut coverage_limitations = mechanism_limitations;
    if let Some(assessment) = &fencing {
        coverage_limitations.push(format!(
            "property audit — {}: {}",
            fira_core::property_audit::FENCING_PROPERTY_TITLE,
            assessment.rationale
        ));
    }
    let coverage = build_coverage(&gates, executed, coverage_limitations, blocked_areas);

    // The report-assembly instant (R1.1): the only clock read for provenance,
    // in the ADAPTER (CORE only formats). A clock failure aborts the run before
    // persistence; no timestamp is fabricated (R1.4).
    let created_at =
        created_at_from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH))?;

    let report = AuditReport {
        schema_version: "1.0.0".to_string(),
        audit_id: request.audit_id.clone().unwrap_or_default(),
        created_at,
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
        determinism_inputs_hash,
    };

    // Persist the canonical report to the external audit workspace (WS-1) by
    // default (§9.4). The ADAPTER renders via PRESENTATION inside the provider;
    // the RUNTIME `WorkspaceSink` publishes the bytes. The project tree is never
    // written. stdout/`--output` remain exported copies, handled by the caller.
    let workspace_base = match args.workspace.clone() {
        Some(base) => base,
        None => resolve_default_base().map_err(AdapterError::WorkspaceUnresolved)?,
    };
    std::fs::create_dir_all(&workspace_base).map_err(|e| {
        AdapterError::Persist(format!(
            "could not create workspace base {}: {e}",
            workspace_base.display()
        ))
    })?;
    let provider = AdapterAuditContextProvider::new(request.clone(), &workspace_base, &root)
        .map_err(|e| AdapterError::Persist(e.to_string()))?;
    let location = provider
        .persist_report(&report)
        .map_err(|e| AdapterError::Persist(e.to_string()))?;

    // Validate the persisted report (vr-enforcement-wiring-slice R1/R3). The
    // report is already written (diagnostic evidence preserved, R3.1); a
    // violation is a fatal adapter error carrying the persisted location and
    // every collected violation (D-11/D-12). On Err the CLI never renders the
    // report (R2.3).
    let violations = validate_report(&report);
    if !violations.is_empty() {
        return Err(AdapterError::VrViolations {
            report_location: location.0.clone(),
            violations,
        });
    }

    Ok(AuditOutput {
        applied,
        view,
        report,
        canonical_location: location.0,
    })
}

/// Run the CORE validators over the assembled report and collect every
/// violation (vr-enforcement-wiring-slice, design §5). Pure: it serializes the
/// report to a `serde_json::Value`, builds the context from it, runs `run_vrs`,
/// VR5 (`ConfidenceRubric`) and VR7 (`VerdictEngine`) in that order, and returns
/// the concatenated violations. It reads no clock, performs no I/O, mutates
/// nothing, and does not know where the report was persisted — mapping the
/// result onto an [`AdapterError`] (with the persisted location) is the caller's
/// job. An empty result means the report passes every wired check.
fn validate_report(report: &AuditReport) -> Vec<Violation> {
    // Serialization cannot fail for this fully-owned, serde-derived type (same
    // precondition PRESENTATION relies on in `render_json`).
    let value = serde_json::to_value(report).expect("AuditReport serializes to a JSON value");

    let ctx = StaticValidationContext::from_report(&value);
    let mut violations = run_vrs(&value, &ctx);
    violations.extend(ConfidenceRubric.recompute_confidence(&value));
    violations.extend(VerdictEngine.recompute_verdict(&value));
    violations
}

/// A `std::time`-derived token used to make each run's `audit_id` distinct by
/// construction (design §2.4). No new crate, no network.
fn audit_nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{seq:x}")
}

/// Pure conversion of a since-epoch clock reading into the S11 `created_at`
/// string (design §6). The caller supplies the reading, so this is testable
/// without a clock. Pre-epoch readings and instants beyond year 9999 are errors
/// (R1.4); no timestamp is fabricated.
fn created_at_from(
    since_epoch: Result<std::time::Duration, std::time::SystemTimeError>,
) -> Result<String, AdapterError> {
    let elapsed = since_epoch.map_err(|_| {
        AdapterError::Provenance("system clock is before the Unix epoch".to_string())
    })?;
    format_utc_timestamp(elapsed.as_secs())
        .ok_or_else(|| AdapterError::Provenance("system clock is beyond year 9999".to_string()))
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
                Ok(
                    AppliedSelection::confirm(proposal)
                        .expect("Classified proposal is confirmable"),
                )
            } else if stdin_is_tty() {
                // An interactive confirm would prompt here; the library default
                // treats a Classified proposal as confirmed only via the caller.
                Ok(
                    AppliedSelection::confirm(proposal)
                        .expect("Classified proposal is confirmable"),
                )
            } else {
                // Non-interactive, no --yes: do not auto-accept.
                Err(AdapterError::NeedsExplicitConfirmation(p))
            }
        }
    }
}

/// Build the executor from the SAME collection that is iterated (R3.6), so the
/// registry can never disagree with the mechanisms the report lists and the
/// filesystem is read once.
fn executor_for(
    mechanisms: &[DiscoveredMechanism],
    root: &std::path::Path,
) -> ProjectCommandExecutor {
    ProjectCommandExecutor::new(MechanismRegistry::from_entries(mechanisms.to_vec()), root)
}

/// Everything the execution step produces, kept together so gate evaluation and
/// coverage read one consistent view.
struct ExecutionPhase {
    /// One `{command_id, outcome}` row per `Ok(ExecutionResult)` (R3.1).
    executed: Vec<ExecutedMechanism>,
    /// The full results with their declared mechanism record (adapter-internal).
    executed_results: Vec<(VerificationMechanism, ExecutionResult)>,
    /// Mechanisms with a `backing_gate` for which `run_existing` returned `Err`.
    /// They never become a result; they only prevent their gate from being PASS.
    without_result: Vec<VerificationMechanism>,
    /// Coverage limitations, in report order.
    limitations: Vec<String>,
}

/// Note for a FAILED mechanism, chosen by the effective classification only.
/// An absent or `UNCLASSIFIED` classification never states an application
/// attribution (R3.3, R6.3).
fn failed_note(id: &str, classification: Option<ExecutionClassification>) -> String {
    match classification {
        None | Some(ExecutionClassification::Unclassified) => format!(
            "Declared verification mechanism '{id}' executed and FAILED; failure is not yet \
             interpreted as an application defect; attribution is not established \
             (UNCLASSIFIED); no application defect has been established."
        ),
        Some(ExecutionClassification::AppLevel) => format!(
            "Declared verification mechanism '{id}' executed and FAILED; classified APP_LEVEL \
             by the execution layer."
        ),
        Some(ExecutionClassification::InfraLevel) => format!(
            "Declared verification mechanism '{id}' executed and FAILED; classified INFRA_LEVEL \
             (infrastructure impediment); not an application-level result."
        ),
    }
}

/// Note for an ERROR mechanism, chosen by the effective classification only.
fn error_note(id: &str, classification: Option<ExecutionClassification>) -> String {
    match classification {
        Some(ExecutionClassification::InfraLevel) => format!(
            "Declared verification mechanism '{id}' could not be started (outcome ERROR, \
             INFRA_LEVEL): the operating system reported that a launcher selected by FIRA \
             could not be found or executed (attribution based on the operating system error \
             kind only; the exact cause is not established); the mechanism was not executed \
             and provides no evidence."
        ),
        None | Some(ExecutionClassification::Unclassified) => format!(
            "Declared verification mechanism '{id}' could not be started (outcome ERROR, \
             UNCLASSIFIED): no positive infrastructure attribution; the mechanism was not \
             executed and provides no evidence."
        ),
        Some(ExecutionClassification::AppLevel) => format!(
            "Declared verification mechanism '{id}' ended with outcome ERROR classified \
             APP_LEVEL by the execution layer."
        ),
    }
}

/// Run every discovered mechanism through `executor`, in discovery order.
///
/// - `Ok(result)`: always a row in `executed` (even ERROR) and an entry in
///   `executed_results`; FAILED/ERROR get a note per classification.
/// - `Err(e)`: a limitation only. It is never turned into a result or a row. If
///   the mechanism backs a gate it is kept in `without_result` so that gate
///   cannot be PASS (R3.5).
///
/// `captured_output` is never read.
fn run_mechanisms(
    executor: &dyn CommandExecutor,
    mechanisms: &[DiscoveredMechanism],
) -> ExecutionPhase {
    let mut executed: Vec<ExecutedMechanism> = Vec::new();
    // Retain the full ExecutionResult + its declared mechanism record so the
    // Tests/Build gate evaluation can read the real outcomes. The coverage
    // `executed_mechanisms` summary is still the `{command_id, outcome}` pair.
    let mut executed_results: Vec<(VerificationMechanism, ExecutionResult)> = Vec::new();
    let mut without_result: Vec<VerificationMechanism> = Vec::new();
    // Task 14: NOT_RUN / TIMEOUT reasons (and start-failure details) surface as
    // coverage limitations (C11: "not sufficiently audited"). Nothing is PASS.
    let mut limitations: Vec<String> = Vec::new();
    // A declared mechanism that FAILS is explicit negative execution evidence;
    // each is a known-limitation entry stating its classification, without
    // asserting an application defect unless the execution layer classified it.
    let mut failed_notes: Vec<String> = Vec::new();
    let mut error_notes: Vec<String> = Vec::new();
    for m in mechanisms {
        match executor.run_existing(&m.mechanism.id) {
            Ok(result) => {
                if let Some(reason) = &result.blocking_condition {
                    limitations.push(format!("{}: {reason}", result.command));
                }
                match result.outcome {
                    ExecutionOutcome::Failed => {
                        failed_notes.push(failed_note(&result.command_id.0, result.classification))
                    }
                    ExecutionOutcome::Error => {
                        error_notes.push(error_note(&result.command_id.0, result.classification))
                    }
                    _ => {}
                }
                executed.push(ExecutedMechanism {
                    command_id: result.command_id.clone(),
                    outcome: result.outcome,
                });
                executed_results.push((m.mechanism.clone(), result));
            }
            Err(e) => {
                let id = &m.mechanism.id.0;
                let mut line = format!(
                    "Declared verification mechanism '{id}' could not be executed: {e}; no \
                     execution result exists."
                );
                if backing_gate(&m.mechanism).is_some() {
                    line.push_str("; the gate it backs cannot be evaluated as PASS");
                    without_result.push(m.mechanism.clone());
                }
                limitations.push(line);
            }
        }
    }
    // One neutral summary line (the failure count) followed by the individual
    // entries, so a skimming reviewer sees the count immediately (spec §9.2).
    if !failed_notes.is_empty() {
        limitations.push(format!(
            "{} declared verification mechanism(s) executed and FAILED; recorded as negative \
             execution evidence; attribution is stated per mechanism.",
            failed_notes.len()
        ));
        limitations.extend(failed_notes);
    }
    limitations.extend(error_notes);
    ExecutionPhase {
        executed,
        executed_results,
        without_result,
        limitations,
    }
}

/// Evaluate the execution-backed gates (Tests, Build) including the backing
/// mechanisms that have no result. The state of each gate is decided by CORE.
fn gates_from_phase(
    profile_gate_levels: &[(GateName, RequirementLevel)],
    phase: &ExecutionPhase,
) -> Vec<Gate> {
    let backed: Vec<ExecutedMechanismResult<'_>> = phase
        .executed_results
        .iter()
        .map(|(mech, result)| ExecutedMechanismResult {
            mechanism: mech,
            result,
        })
        .collect();
    let without_result: Vec<&VerificationMechanism> = phase.without_result.iter().collect();
    evaluate_execution_backed_gates_with_unexecuted(profile_gate_levels, &backed, &without_result)
}

/// Fixed detail used when an INFRA result carries no `blocking_condition`.
const INFRA_BLOCKED_FALLBACK: &str =
    "infrastructure impediment reported by the execution layer; no further detail recorded";

/// `blocked_areas` for results classified `INFRA_LEVEL` (outcome ERROR, FAILED
/// or BLOCKED). An entry exists only when the mechanism backs a gate that is
/// present in the report and applicable; the `area` is that gate's name. A
/// mechanism without a gate (or whose gate is absent or N/A) yields no entry and
/// no invented area: its ERROR row and limitation remain.
fn infra_blocked_areas(
    executed_results: &[(VerificationMechanism, ExecutionResult)],
    gates: &[Gate],
) -> Vec<BlockedArea> {
    executed_results
        .iter()
        .filter(|(_, r)| {
            r.classification == Some(ExecutionClassification::InfraLevel)
                && matches!(
                    r.outcome,
                    ExecutionOutcome::Error | ExecutionOutcome::Failed | ExecutionOutcome::Blocked
                )
        })
        .filter_map(|(mech, r)| {
            let gate = backing_gate(mech)?;
            gates
                .iter()
                .find(|g| g.name == gate && g.state != GateState::NotApplicable)?;
            Some(BlockedArea {
                area: gate_wire(&gate),
                blocking_condition: r
                    .blocking_condition
                    .clone()
                    .unwrap_or_else(|| INFRA_BLOCKED_FALLBACK.to_string()),
                execution_result_ref: r.command_id.clone(),
            })
        })
        .collect()
}

/// Whether the canonical `npm-test` mechanism executed and PASSED (level 4 — the
/// mechanism-OBSERVED fact). This is NOT, on its own, evidence for the fencing
/// property (that would be the forbidden inference); it is passed to the CORE
/// classifier only as the honest "the suite ran and passed" input.
fn npm_test_passed(executed: &[(VerificationMechanism, ExecutionResult)]) -> bool {
    executed
        .iter()
        .any(|(m, r)| m.id.0 == "npm-test" && r.outcome == ExecutionOutcome::Passed)
}

/// P2/P3/P6 for the one targeted property (the JournalStore recovery-generation
/// fencing invariant). Reads the targeted source via the READ capability only
/// (no narrative, no project write), detects conservative presence signals, and
/// classifies them with the pure CORE classifier. Returns `None` when the
/// project is not store-shaped (the targeted interface file is absent), so the
/// audit is a no-op on unrelated projects.
///
/// Option A: `exercised` is always `NotEstablished` — FIRA has no grounded way
/// (without captured-output parsing, which is out of scope) to show the specific
/// fencing behavior ran, so the property's ceiling is UNVERIFIED even when the
/// test mechanism PASSED. Nothing here parses command output or matches test
/// names.
fn audit_fencing_property(
    reader: &FsRepositoryReader,
    mechanism_passed: bool,
) -> Option<FencingAssessment> {
    // The targeted store interface. Its absence ⇒ not a store-shaped project ⇒
    // this property does not apply here (no-op).
    let interface = reader.read_file("src/stores/interface.ts", None).ok()?;

    // Level 1 (SPECIFIED): the interface declares the fencing contract. The
    // guarantee is extracted from the code's own contract (C3), via conservative
    // enumerated signals (the `expectedGeneration` parameter + the FENCED
    // rejection the contract names).
    let specified_in_interface =
        interface.contains("expectedGeneration") && interface.contains("FENCED");

    // Level 2 (IMPLEMENTED): a mutating write in the implementation guards on the
    // persisted recovery_generation (the CAS predicate). Read-only; absence is
    // recorded honestly (defect-variant signal).
    let implemented_guard_present = reader
        .read_file("src/stores/sqlite.ts", None)
        .ok()
        .map(|impl_src| impl_src.contains("recovery_generation"))
        .unwrap_or(false);

    // Level 3 (TESTED, static): a test body appears to assert stale-generation
    // rejection (a fenced write is rejected). Reading test code is permitted
    // static evidence; it is TESTED-not-executed (never VERIFIED on its own).
    let tested_rejection_present = reader
        .read_file("tests/stores/claim-recovery.test.ts", None)
        .ok()
        .map(|test_src| test_src.contains("FENCED") || test_src.contains("fenced"))
        .unwrap_or(false);

    let evidence = FencingEvidence {
        specified_in_interface,
        implemented_guard_present,
        tested_rejection_present,
        mechanism_passed,
        // Option A: never established (no captured-output parsing, by ruling).
        exercised: PropertyExercised::NotEstablished,
    };
    Some(classify_fencing(&evidence))
}

/// Record each profile gate at its honest "not yet interpreted" state: UNKNOWN
/// for applicable gates, N/A for profile-N/A gates. No evidence-based derivation.
fn build_unevaluated_gates(profile: &fira_policy::AuditProfile) -> Vec<Gate> {
    profile
        .gates
        .iter()
        .map(|g| {
            let (state, cause) = match g.requirement_level {
                RequirementLevel::NotApplicable => {
                    (GateState::NotApplicable, Some(GateCause::None))
                }
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
fn build_coverage(
    gates: &[Gate],
    executed: Vec<ExecutedMechanism>,
    mechanism_limitations: Vec<String>,
    blocked_areas: Vec<BlockedArea>,
) -> CoverageStatement {
    // A gate the slice actually evaluated (PASS/FAIL) is an *audited* area;
    // every other gate remains a *skipped* area ("not sufficiently audited").
    // Every gate still appears in exactly one coverage entry (VR8).
    let mut audited_areas: Vec<AuditedArea> = Vec::new();
    let mut skipped_areas: Vec<SkippedArea> = Vec::new();
    for g in gates {
        match g.state {
            GateState::Pass => audited_areas.push(AuditedArea {
                area: gate_wire(&g.name),
                depth: Depth::Standard,
                result: CoverageResult::NoIssueFound,
            }),
            GateState::Fail => audited_areas.push(AuditedArea {
                area: gate_wire(&g.name),
                depth: Depth::Standard,
                result: CoverageResult::IssuesFound,
            }),
            // A gate represented in `blocked_areas` is not also skipped: each
            // gate stays in exactly one of the three lists (R5.3).
            _ => {
                let area = gate_wire(&g.name);
                if !blocked_areas.iter().any(|b| b.area == area) {
                    skipped_areas.push(SkippedArea {
                        area,
                        reason: "audit interpretation (P2-P11) not yet performed".to_string(),
                    });
                }
            }
        }
    }
    let mut known_limitations = vec![
        "Task 10 minimal run: mechanisms discovered and executed, but evidence \
         interpretation, finding derivation, and gate-state derivation are not \
         yet implemented."
            .to_string(),
    ];
    // Task 14: surface each not-executed (NOT_RUN) / timed-out (TIMEOUT)
    // mechanism and its reason, so the report distinguishes "not sufficiently
    // audited" from "no issue found" (C11). These never imply PASS (C5).
    known_limitations.extend(mechanism_limitations);
    CoverageStatement {
        audited_areas,
        skipped_areas,
        blocked_areas,
        executed_mechanisms: executed,
        known_limitations,
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

#[cfg(test)]
mod tests {
    use super::{
        created_at_from, validate_report, AdapterAuditContextProvider, AdapterError,
        AuditContextProvider, AuditReport, Violation,
    };
    use fira_core::validation::Vr;
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    /// AC-9: sub-second part truncated; fixed input, no clock read.
    #[test]
    fn created_at_from_formats_fixed_duration() {
        let d = Duration::from_secs(1_767_225_599) + Duration::from_millis(999);
        assert_eq!(created_at_from(Ok(d)).unwrap(), "2025-12-31T23:59:59Z");
    }

    /// AC-9: a pre-epoch reading is an explicit provenance error. The error is
    /// built deterministically from two fixed instants (no clock read).
    #[test]
    fn created_at_from_rejects_pre_epoch() {
        let pre_epoch = UNIX_EPOCH.duration_since(UNIX_EPOCH + Duration::from_secs(1));
        assert!(pre_epoch.is_err(), "fixture must be a pre-epoch error");
        assert!(matches!(
            created_at_from(pre_epoch),
            Err(AdapterError::Provenance(_))
        ));
    }

    /// AC-9: an instant beyond year 9999 is an explicit provenance error.
    #[test]
    fn created_at_from_rejects_beyond_year_9999() {
        assert!(matches!(
            created_at_from(Ok(Duration::from_secs(253_402_300_800))),
            Err(AdapterError::Provenance(_))
        ));
    }

    /// The new variant's Display text (design §6).
    #[test]
    fn provenance_error_display() {
        let e = AdapterError::Provenance("x".to_string());
        assert_eq!(e.to_string(), "failed to establish report provenance: x");
    }

    // ---------------------------------------------------------------------
    // vr-enforcement-wiring-slice, Task 5 (AC-3, AC-4): `validate_report`
    // detection and aggregation. A real `run_audit` report passes every wired
    // rule, so violations are provoked on in-memory copies of the S11 golden.
    // The golden is embedded read-only at compile time and is never written.
    // ---------------------------------------------------------------------

    const GOLDEN: &str = include_str!("../../../schemas/examples/valid/s11-audit-report.json");

    fn golden_value() -> Value {
        serde_json::from_str(GOLDEN).expect("golden S11 report parses as JSON")
    }

    /// Deserialize as `AuditReport` first (so a mutation that is not
    /// structurally valid fails loudly instead of skipping the validators),
    /// then run the function under test.
    fn validate_value(value: Value) -> Vec<Violation> {
        let report: AuditReport = serde_json::from_value(value)
            .expect("mutated golden still deserializes as AuditReport");
        validate_report(&report)
    }

    fn count_of(violations: &[Violation], vr: Vr) -> usize {
        violations.iter().filter(|v| v.vr == vr).count()
    }

    /// VR8 mutation: the only gate (`Tests`) loses its coverage entry.
    fn mutate_vr8(value: &mut Value) {
        let areas = value["coverage"]["audited_areas"]
            .as_array_mut()
            .expect("golden has coverage.audited_areas");
        let before = areas.len();
        areas.retain(|a| a["area"].as_str() != Some("Tests"));
        assert_eq!(
            before - areas.len(),
            1,
            "mutation must remove the Tests entry"
        );
    }

    /// VR5 mutation: the HIGH finding RR-001 loses its execution-based facet.
    ///
    /// Only `epistemic_state.facets` (OBSERVED removed) and
    /// `epistemic_state.conclusion` (VERIFIED -> UNVERIFIED, so VR11 does not
    /// also fire) change; `confidence` stays HIGH. `confidence_derivation` is
    /// intentionally left stale ("execution-based facet present ...") because it
    /// is a descriptive audit-trail string that no rule evaluates; editing it
    /// would only widen the mutation without affecting what VR5 checks.
    fn mutate_vr5(value: &mut Value) {
        let finding = &mut value["findings"][0];
        assert_eq!(finding["id"], "RR-001", "golden finding precondition");
        assert_eq!(
            finding["confidence"], "HIGH",
            "golden confidence precondition"
        );
        assert!(
            finding["epistemic_state"]["facets"]
                .as_array()
                .expect("facets array")
                .iter()
                .any(|f| f == "OBSERVED"),
            "golden precondition: RR-001 carries OBSERVED"
        );
        finding["epistemic_state"]["facets"] = json!(["IMPLEMENTED", "TESTED"]);
        finding["epistemic_state"]["conclusion"] = json!("UNVERIFIED");
    }

    /// VR7 mutation: the recorded result no longer matches the recomputation.
    fn mutate_vr7(value: &mut Value) {
        assert_eq!(
            value["technical_assessment"]["result"], "READY",
            "golden precondition: recorded result is READY"
        );
        value["technical_assessment"]["result"] = json!("NOT_READY");
    }

    /// Baseline: the unmodified golden passes every wired check.
    #[test]
    fn validate_report_golden_baseline_is_clean() {
        let violations = validate_value(golden_value());
        assert!(
            violations.is_empty(),
            "unexpected violations: {violations:?}"
        );
    }

    /// AC-3a: `run_vrs` (VR8) is wired.
    #[test]
    fn validate_report_detects_vr8_gate_without_coverage() {
        let mut value = golden_value();
        mutate_vr8(&mut value);
        let violations = validate_value(value);
        assert_eq!(violations.len(), 1, "got: {violations:?}");
        assert_eq!(violations[0].vr, Vr::Vr8);
        assert_eq!(violations[0].locator, "gates[0]");
    }

    /// AC-3b: VR5 (`ConfidenceRubric`) is wired; the detail names the rubric
    /// branch (`PublishableNonHigh`), so it is the HIGH-without-evidence
    /// overclaim and not the NotPublishable branch.
    #[test]
    fn validate_report_detects_vr5_high_confidence_without_execution_facet() {
        let mut value = golden_value();
        mutate_vr5(&mut value);
        let violations = validate_value(value);
        assert_eq!(violations.len(), 1, "got: {violations:?}");
        assert_eq!(violations[0].vr, Vr::Vr5);
        assert_eq!(violations[0].locator, "findings[0]");
        assert!(
            violations[0].detail.contains("PublishableNonHigh"),
            "detail: {}",
            violations[0].detail
        );
    }

    /// AC-3c: VR7 (`VerdictEngine`) is wired.
    #[test]
    fn validate_report_detects_vr7_recorded_result_mismatch() {
        let mut value = golden_value();
        mutate_vr7(&mut value);
        let violations = validate_value(value);
        assert_eq!(violations.len(), 1, "got: {violations:?}");
        assert_eq!(violations[0].vr, Vr::Vr7);
        assert_eq!(violations[0].locator, "technical_assessment.result");
    }

    /// AC-4: violations from `run_vrs` and VR7 are collected together. Presence
    /// and total count only; no dependence on the order of the list.
    #[test]
    fn validate_report_accumulates_vr8_and_vr7() {
        let mut value = golden_value();
        mutate_vr8(&mut value);
        mutate_vr7(&mut value);
        let violations = validate_value(value);
        assert_eq!(violations.len(), 2, "got: {violations:?}");
        assert_eq!(count_of(&violations, Vr::Vr8), 1);
        assert_eq!(count_of(&violations, Vr::Vr7), 1);
    }

    /// AC-4: all three wired sources (`run_vrs`, VR5, VR7) contribute to one
    /// list. Presence and total count only.
    #[test]
    fn validate_report_accumulates_vr8_vr5_and_vr7() {
        let mut value = golden_value();
        mutate_vr8(&mut value);
        mutate_vr5(&mut value);
        mutate_vr7(&mut value);
        let violations = validate_value(value);
        assert_eq!(violations.len(), 3, "got: {violations:?}");
        assert_eq!(count_of(&violations, Vr::Vr8), 1);
        assert_eq!(count_of(&violations, Vr::Vr5), 1);
        assert_eq!(count_of(&violations, Vr::Vr7), 1);
    }

    // ---------------------------------------------------------------------
    // vr-enforcement-wiring-slice, Task 6 (AC-11): content of the
    // `AdapterError::VrViolations` message. The variant is built directly, so
    // no violating project is needed. The assertions are about literal content
    // and about nothing being lost or duplicated; they deliberately do not pin
    // the header wording, the bullet format or the order of the list.
    // ---------------------------------------------------------------------

    const REPORT_LOCATION: &str = "/tmp/fira audit ws/audit-v1-0123-0";

    /// Struct literal: `Violation::new` is `pub(crate)` in fira-core, but the
    /// fields are public.
    fn violation(vr: Vr, locator: &str, detail: &str) -> Violation {
        Violation {
            vr,
            locator: locator.to_string(),
            detail: detail.to_string(),
        }
    }

    /// The `Display` text of the error under test.
    fn error_text(location: &str, violations: Vec<Violation>) -> String {
        AdapterError::VrViolations {
            report_location: location.to_string(),
            violations,
        }
        .to_string()
    }

    /// Real violations produced by the Task 5 mutations: VR8 (`run_vrs`), VR5
    /// and VR7.
    fn triple_violations() -> Vec<Violation> {
        let mut value = golden_value();
        mutate_vr8(&mut value);
        mutate_vr5(&mut value);
        mutate_vr7(&mut value);
        validate_value(value)
    }

    /// The violation's composed `Display` (id, locator and detail together)
    /// appears exactly `times` times in `text`.
    fn assert_composed_count(text: &str, v: &Violation, times: usize) {
        assert_eq!(
            text.matches(&v.to_string()).count(),
            times,
            "{} at {} should appear {times} time(s)",
            v.vr.id(),
            v.locator
        );
    }

    /// AC-11 (a)(b)(c): the location, and every violation complete and listed
    /// exactly once. The id is checked with `contains` only: a detail can
    /// mention another id (the VR5 detail contains "VR5"), so counting ids would
    /// be wrong; the composed string is what pins id, locator and detail
    /// together.
    #[test]
    fn vr_violations_error_lists_location_and_every_violation() {
        let violations = triple_violations();
        // Guards: the input meets the AC-11 minimum (>= 3 violations, >= 2 ids).
        assert!(violations.len() >= 3, "got: {violations:?}");
        let mut ids: Vec<&str> = violations.iter().map(|v| v.vr.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert!(ids.len() >= 2, "got: {ids:?}");

        let text = error_text(REPORT_LOCATION, violations.clone());

        assert!(text.contains(REPORT_LOCATION), "location missing: {text}");
        for v in &violations {
            assert!(text.contains(v.vr.id()), "id missing for {v}");
            assert!(text.contains(&v.locator), "locator missing for {v}");
            assert!(text.contains(&v.detail), "detail missing for {v}");
            assert_composed_count(&text, v, 1);
        }
    }

    /// AC-11 (c), D-12: no cap on the number of violations. 100 is above the
    /// usual limits (10, 20, 50); a cap of 100 or more would not be detected.
    #[test]
    fn vr_violations_error_has_no_violation_cap() {
        const COUNT: usize = 100;
        let ids = [
            Vr::Vr1,
            Vr::Vr2,
            Vr::Vr5,
            Vr::Vr7,
            Vr::Vr8,
            Vr::Vr9,
            Vr::Vr11,
            Vr::Vr12,
            Vr::Vr13,
        ];
        // Fixed-width numbers, so no locator or detail is a substring of another.
        let violations: Vec<Violation> = (0..COUNT)
            .map(|i| {
                violation(
                    ids[i % ids.len()],
                    &format!("findings[{i:03}]"),
                    &format!("detail-{i:03}"),
                )
            })
            .collect();
        assert_eq!(violations.len(), COUNT);

        let text = error_text(REPORT_LOCATION, violations.clone());

        for v in &violations {
            assert_eq!(
                text.matches(&v.detail).count(),
                1,
                "detail of {} should be listed exactly once",
                v.locator
            );
            assert_composed_count(&text, v, 1);
        }
    }

    /// R2.1a: no deduplication. Two identical violations are both listed.
    #[test]
    fn vr_violations_error_does_not_deduplicate() {
        let first = violation(
            Vr::Vr8,
            "gates[0]",
            "gate `Tests` does not appear in any coverage entry",
        );
        let second = violation(
            Vr::Vr8,
            "gates[0]",
            "gate `Tests` does not appear in any coverage entry",
        );
        let other = violation(
            Vr::Vr7,
            "technical_assessment.result",
            "recorded result differs from recomputed",
        );
        assert_eq!(first, second, "guard: the two violations are identical");

        let text = error_text(REPORT_LOCATION, vec![first.clone(), second, other.clone()]);

        assert_composed_count(&text, &first, 2);
        assert_composed_count(&text, &other, 1);
    }

    /// AC-11 (b): content is rendered verbatim, with nothing escaped or cut.
    /// One list, one violation per kind of content, so a list cut to its first
    /// element is detected too.
    #[test]
    fn vr_violations_error_preserves_content_verbatim() {
        let long = format!("START-{}-END", "a".repeat(9_990));
        assert_eq!(long.chars().count(), 10_000, "guard: detail length");
        let violations = vec![
            violation(Vr::Vr9, "findings[0].title", "ñandú → 検証 ✓ 🦀"),
            violation(
                Vr::Vr7,
                "technical_assessment.result",
                "line1\nline2\r\nline3\n",
            ),
            violation(
                Vr::Vr5,
                "findings[1]",
                r#"double " single ' tick ` backslash \ and \n literal"#,
            ),
            violation(Vr::Vr1, "gates[2]", "{} {0} {name} {{x}} }{"),
            violation(Vr::Vr2, "findings[3]", &long),
        ];

        let text = error_text(REPORT_LOCATION, violations.clone());

        for v in &violations {
            assert!(
                text.contains(&v.detail),
                "detail of {} altered or missing",
                v.locator
            );
            assert_composed_count(&text, v, 1);
        }
    }

    // ---------------------------------------------------------------------
    // vr-enforcement-wiring-slice, Task 7 (AC-5, R3.1, R3.2): a report is
    // persisted whether or not it violates a rule, and the persisted file is
    // the exact report that was validated. The two calls are composed here, in
    // the order `run_audit` uses (persist, then `validate_report`). These tests
    // do NOT prove that `run_audit` itself keeps that order: a violating
    // project cannot be driven through it, so the order is verified by code
    // inspection (AC-1).
    // ---------------------------------------------------------------------

    /// A report persisted into throw-away directories. Both temp dirs are kept
    /// alive here: they must exist and be disjoint for the workspace sink.
    struct Persisted {
        _project: tempfile::TempDir,
        _workspace: tempfile::TempDir,
        /// The canonical workspace base used by the provider.
        base: PathBuf,
        /// The location returned by `persist_report`.
        dir: PathBuf,
    }

    /// Persist `report` through the same `AdapterAuditContextProvider` /
    /// `persist_report` path `run_audit` uses. Persistence takes no part in
    /// validation, so it must succeed for any report.
    fn persist_in_temp_workspace(report: &AuditReport) -> Persisted {
        let project = tempfile::tempdir().expect("project tempdir");
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let provider = AdapterAuditContextProvider::new(
            report.request.clone(),
            workspace.path(),
            project.path(),
        )
        .expect("workspace is outside the project tree");
        let location = provider
            .persist_report(report)
            .expect("report is persisted regardless of its validity");
        Persisted {
            base: provider.workspace_base().to_path_buf(),
            dir: PathBuf::from(location.0),
            _project: project,
            _workspace: workspace,
        }
    }

    /// Read `report.json` back from the persisted location and deserialize it
    /// with the inverse of the renderer used to write it.
    fn load_persisted(persisted: &Persisted) -> AuditReport {
        let json = persisted.dir.join("report.json");
        assert!(json.is_file(), "report.json missing at {}", json.display());
        let text = std::fs::read_to_string(&json).expect("report.json is readable");
        fira_presentation::parse_json(&text).expect("report.json deserializes as AuditReport")
    }

    /// AC-5, R3.1, R3.2: a report that violates a rule is still persisted, the
    /// file is the exact report that was validated, and it reproduces the same
    /// violations when validated again (so it is usable as diagnostic evidence).
    #[test]
    fn persisted_report_survives_vr_violations() {
        let mut value = golden_value();
        mutate_vr8(&mut value);
        let report: AuditReport =
            serde_json::from_value(value).expect("mutated golden deserializes as AuditReport");

        // Persist first, validate after: the order `run_audit` uses.
        let persisted = persist_in_temp_workspace(&report);
        let violations = validate_report(&report);

        // Guard: the input really violates a rule (exactly the VR8 mutation).
        assert_eq!(violations.len(), 1, "got: {violations:?}");
        assert_eq!(violations[0].vr, Vr::Vr8);

        assert_eq!(persisted.dir, persisted.base.join(&report.audit_id));
        let reloaded = load_persisted(&persisted);
        assert_eq!(reloaded, report, "persisted report differs from validated");
        assert_eq!(
            validate_report(&reloaded),
            violations,
            "persisted report must reproduce the same violations"
        );
    }

    /// AC-5 (clean case): a report with no violations is persisted in the same
    /// way and the file is the exact report that was validated.
    #[test]
    fn persisted_clean_report_validates_clean() {
        let report: AuditReport =
            serde_json::from_value(golden_value()).expect("golden deserializes as AuditReport");

        let persisted = persist_in_temp_workspace(&report);
        let violations = validate_report(&report);

        assert!(violations.is_empty(), "got: {violations:?}");
        assert_eq!(persisted.dir, persisted.base.join(&report.audit_id));
        assert_eq!(
            load_persisted(&persisted),
            report,
            "persisted report differs from validated"
        );
    }

    // ---------------------------------------------------------------------
    // failure-classification-slice, Task 4: evidence preservation, coherent
    // registry and `blocked_areas`. A stub `CommandExecutor` supplies any
    // `(outcome, classification)` or `Err`, including shapes that real
    // executions cannot reach. No process is started.
    // ---------------------------------------------------------------------

    use super::{
        build_coverage, error_note, executor_for, failed_note, gate_wire, gates_from_phase,
        infra_blocked_areas, run_mechanisms, ExecutionPhase, INFRA_BLOCKED_FALLBACK,
    };
    use fira_core::coverage::BlockedArea;
    use fira_core::execution::{ExecutionResult, VerificationMechanism};
    use fira_core::gate::Gate;
    use fira_core::interfaces::{CapabilityError, CommandExecutor};
    use fira_core::model::{
        AlwaysTrue, Capability, ExecutionClassification as Class, ExecutionOutcome as Outcome,
        GateCause, GateName, GateState, IdRef, RequirementLevel, VerificationMechanismSource,
    };
    use fira_runtime::DiscoveredMechanism;
    use std::collections::{BTreeMap, BTreeSet};

    fn discovered(id: &str, non_terminating: Option<&str>) -> DiscoveredMechanism {
        DiscoveredMechanism {
            mechanism: VerificationMechanism {
                id: IdRef(id.to_string()),
                source: VerificationMechanismSource::Make,
                source_locator: "Makefile:1".to_string(),
                command: format!("make {id}"),
                declared_by_project: AlwaysTrue,
            },
            argv: vec!["make".to_string(), id.to_string()],
            non_terminating_reason: non_terminating.map(str::to_string),
        }
    }

    fn exec_result(
        id: &str,
        outcome: Outcome,
        classification: Option<Class>,
        blocking: Option<&str>,
    ) -> ExecutionResult {
        ExecutionResult {
            command_id: IdRef(id.to_string()),
            command: format!("make {id}"),
            outcome,
            classification,
            blocking_condition: blocking.map(str::to_string),
            captured_output: String::new(),
            exercised_behaviors: Vec::new(),
        }
    }

    /// Answers `run_existing` from a fixed table; an id not in the table is
    /// `NotFound`, like the real executor.
    struct StubExecutor {
        answers: BTreeMap<String, Result<ExecutionResult, CapabilityError>>,
    }

    impl StubExecutor {
        fn new(answers: Vec<(&str, Result<ExecutionResult, CapabilityError>)>) -> Self {
            StubExecutor {
                answers: answers
                    .into_iter()
                    .map(|(id, a)| (id.to_string(), a))
                    .collect(),
            }
        }
    }

    impl CommandExecutor for StubExecutor {
        fn run_existing(&self, command_id: &IdRef) -> Result<ExecutionResult, CapabilityError> {
            self.answers
                .get(&command_id.0)
                .cloned()
                .unwrap_or_else(|| Err(CapabilityError::NotFound(command_id.0.clone())))
        }

        fn capabilities(&self) -> BTreeSet<Capability> {
            BTreeSet::new()
        }

        fn discover_mechanisms(&self) -> Result<Vec<VerificationMechanism>, CapabilityError> {
            Ok(Vec::new())
        }
    }

    fn levels() -> Vec<(GateName, RequirementLevel)> {
        vec![
            (GateName::Build, RequirementLevel::Required),
            (GateName::Tests, RequirementLevel::Required),
        ]
    }

    fn run(
        mechs: &[DiscoveredMechanism],
        answers: Vec<(&str, Result<ExecutionResult, CapabilityError>)>,
    ) -> ExecutionPhase {
        run_mechanisms(&StubExecutor::new(answers), mechs)
    }

    fn gate_of(gates: &[Gate], name: GateName) -> &Gate {
        gates.iter().find(|g| g.name == name).expect("gate present")
    }

    fn unknown_gate(name: GateName) -> Gate {
        Gate {
            name,
            requirement_level: RequirementLevel::Required,
            state: GateState::Unknown,
            rationale: "x".to_string(),
            cause: Some(GateCause::None),
            support_mappings: Vec::new(),
            supporting_findings: Vec::new(),
        }
    }

    /// The same mechanism written as a (declared record, result) pair.
    fn pair(
        id: &str,
        outcome: Outcome,
        classification: Option<Class>,
        blocking: Option<&str>,
    ) -> (VerificationMechanism, ExecutionResult) {
        (
            discovered(id, None).mechanism,
            exec_result(id, outcome, classification, blocking),
        )
    }

    // ----- AC-26: an `Err` is a limitation, never a result -----

    /// R3.2: the limitation names the mechanism and the error.
    #[test]
    fn err_from_executor_is_recorded_as_limitation() {
        let mechs = [discovered("make-test", None)];
        let phase = run(
            &mechs,
            vec![(
                "make-test",
                Err(CapabilityError::Failed("boom".to_string())),
            )],
        );
        let line = phase
            .limitations
            .iter()
            .find(|l| l.contains("'make-test'"))
            .expect("limitation names the mechanism");
        assert!(line.contains("could not be executed"), "{line}");
        assert!(line.contains("boom"), "the error is named: {line}");
        assert!(line.contains("no execution result exists"), "{line}");
    }

    /// R3.5: a sibling PASSED cannot hide a backing mechanism with no result.
    #[test]
    fn err_on_backing_mechanism_blocks_pass_even_with_passed_sibling() {
        let mechs = [discovered("make-test", None), discovered("npm-test", None)];
        let phase = run(
            &mechs,
            vec![
                (
                    "make-test",
                    Ok(exec_result("make-test", Outcome::Passed, None, None)),
                ),
                ("npm-test", Err(CapabilityError::Failed("boom".to_string()))),
            ],
        );
        assert_eq!(phase.without_result.len(), 1);
        assert_eq!(phase.without_result[0].id.0, "npm-test");
        let line = phase
            .limitations
            .iter()
            .find(|l| l.contains("'npm-test'"))
            .expect("limitation");
        assert!(line.contains("the gate it backs cannot be evaluated as PASS"));

        let gates = gates_from_phase(&levels(), &phase);
        let tests = gate_of(&gates, GateName::Tests);
        assert_eq!(tests.state, GateState::Unknown);
        assert_eq!(tests.cause, Some(GateCause::None));
        assert!(
            tests.rationale.contains("could not be executed"),
            "{}",
            tests.rationale
        );
    }

    /// R3.2: no row, no result and no ERROR outcome come from an `Err`.
    #[test]
    fn err_is_not_converted_to_error_result() {
        let mechs = [
            discovered("make-test", None),
            discovered("make-build", None),
        ];
        let phase = run(
            &mechs,
            vec![
                (
                    "make-test",
                    Err(CapabilityError::Failed("boom".to_string())),
                ),
                (
                    "make-build",
                    Err(CapabilityError::NotFound("make-build".to_string())),
                ),
            ],
        );
        assert!(phase.executed.is_empty(), "no executed_mechanisms row");
        assert!(phase.executed_results.is_empty(), "no ExecutionResult");
        assert!(phase.executed.iter().all(|r| r.outcome != Outcome::Error));
        assert_eq!(phase.without_result.len(), 2);
        assert_eq!(phase.limitations.len(), 2);
    }

    /// A mechanism that backs no gate only adds the limitation.
    #[test]
    fn err_on_non_backing_mechanism_only_adds_limitation() {
        let mechs = [discovered("make-lint", None), discovered("make-test", None)];
        let phase = run(
            &mechs,
            vec![
                (
                    "make-lint",
                    Err(CapabilityError::Failed("boom".to_string())),
                ),
                (
                    "make-test",
                    Ok(exec_result("make-test", Outcome::Passed, None, None)),
                ),
            ],
        );
        assert!(phase.without_result.is_empty());
        assert_eq!(phase.limitations.len(), 1);
        assert!(phase.limitations[0].contains("'make-lint'"));
        assert!(!phase.limitations[0].contains("the gate it backs"));
        // The sibling keeps its PASS: nothing else changed.
        let gates = gates_from_phase(&levels(), &phase);
        assert_eq!(gate_of(&gates, GateName::Tests).state, GateState::Pass);
    }

    // ----- AC-30: one collection feeds both the executor and the iteration -----

    #[test]
    fn executor_registry_matches_discovered_collection() {
        // Non-terminating entries return NOT_RUN without starting a process.
        let mechs = [
            discovered("make-test", Some("watcher")),
            discovered("custom-one", Some("watcher")),
        ];
        let root = std::env::temp_dir();
        let executor = executor_for(&mechs, &root);

        let ids: Vec<String> = executor
            .discover_mechanisms()
            .expect("registry listing")
            .into_iter()
            .map(|m| m.id.0)
            .collect();
        let expected: Vec<String> = mechs.iter().map(|m| m.mechanism.id.0.clone()).collect();
        assert_eq!(ids, expected, "exactly the ids of the given collection");

        for m in &mechs {
            let r = executor
                .run_existing(&m.mechanism.id)
                .expect("every given id resolves");
            assert_eq!(r.outcome, Outcome::NotRun);
            assert_eq!(r.command_id, m.mechanism.id);
        }
        assert!(executor
            .run_existing(&IdRef("not-in-collection".to_string()))
            .is_err());
    }

    // ----- notes depend on the classification only -----

    #[test]
    fn failed_notes_follow_classification() {
        for c in [None, Some(Class::Unclassified)] {
            let n = failed_note("m", c);
            for needle in [
                "FAILED",
                "not yet interpreted",
                "attribution is not established",
                "no application defect has been established",
            ] {
                assert!(n.contains(needle), "{needle}: {n}");
            }
            assert!(!n.contains("APP_LEVEL"), "{n}");
        }
        let app = failed_note("m", Some(Class::AppLevel));
        assert!(app.contains("classified APP_LEVEL by the execution layer"));
        let infra = failed_note("m", Some(Class::InfraLevel));
        assert!(infra.contains("INFRA_LEVEL") && infra.contains("not an application-level"));
        assert!(!infra.contains("APP_LEVEL"));
    }

    #[test]
    fn error_notes_follow_classification() {
        for c in [None, Some(Class::Unclassified)] {
            let n = error_note("m", c);
            assert!(n.contains("UNCLASSIFIED"), "{n}");
            assert!(n.contains("no positive infrastructure attribution"), "{n}");
            assert!(n.contains("provides no evidence"), "{n}");
            assert!(
                !n.contains("INFRA_LEVEL") && !n.contains("APP_LEVEL"),
                "{n}"
            );
        }
        let app = error_note("m", Some(Class::AppLevel));
        assert!(app.contains("classified APP_LEVEL by the execution layer"));

        // End to end through `run_mechanisms`: the note follows the result.
        let mechs = [
            discovered("make-test", None),
            discovered("make-build", None),
        ];
        let phase = run(
            &mechs,
            vec![
                (
                    "make-test",
                    Ok(exec_result("make-test", Outcome::Error, None, Some("d1"))),
                ),
                (
                    "make-build",
                    Ok(exec_result(
                        "make-build",
                        Outcome::Error,
                        Some(Class::Unclassified),
                        Some("d2"),
                    )),
                ),
            ],
        );
        assert_eq!(
            phase
                .limitations
                .iter()
                .filter(|l| l.contains("could not be started (outcome ERROR, UNCLASSIFIED)"))
                .count(),
            2
        );
    }

    /// AC-29 (adapter part): the INFRA note does not claim the exact cause.
    #[test]
    fn error_infra_note_is_hedged() {
        let n = error_note("make-test", Some(Class::InfraLevel));
        assert!(n.contains("INFRA_LEVEL"));
        assert!(n.contains("attribution based on the operating system error kind only"));
        assert!(n.contains("exact cause is not established"));
        assert!(n.contains("was not executed and provides no evidence"));
    }

    /// The count line makes no attribution claim, whatever the classifications.
    #[test]
    fn summary_line_is_attribution_neutral() {
        let mechs = [
            discovered("make-test", None),
            discovered("make-build", None),
            discovered("make-lint", None),
        ];
        let phase = run(
            &mechs,
            vec![
                (
                    "make-test",
                    Ok(exec_result(
                        "make-test",
                        Outcome::Failed,
                        Some(Class::AppLevel),
                        None,
                    )),
                ),
                (
                    "make-build",
                    Ok(exec_result("make-build", Outcome::Failed, None, None)),
                ),
                (
                    "make-lint",
                    Ok(exec_result(
                        "make-lint",
                        Outcome::Failed,
                        Some(Class::Unclassified),
                        None,
                    )),
                ),
            ],
        );
        let summary_pos = phase
            .limitations
            .iter()
            .position(|l| l.contains("declared verification mechanism(s) executed and FAILED"))
            .expect("summary line present");
        let summary = &phase.limitations[summary_pos];
        assert!(summary.starts_with("3 declared"), "{summary}");
        assert!(summary.contains("attribution is stated per mechanism"));
        for forbidden in ["APP", "INFRA", "UNCLASSIFIED", "application defect"] {
            assert!(!summary.contains(forbidden), "{forbidden}: {summary}");
        }
        // The summary comes first, followed by exactly one note per failure.
        let notes = &phase.limitations[summary_pos + 1..];
        assert_eq!(notes.len(), 3);
        assert!(notes[0].contains("'make-test'") && notes[0].contains("APP_LEVEL"));
        assert!(notes[1].contains("'make-build'") && notes[1].contains("UNCLASSIFIED"));
    }

    /// Unaffected mechanisms keep their behavior: no note, no extra limitation.
    #[test]
    fn passed_timeout_and_not_run_add_no_notes() {
        let mechs = [
            discovered("make-test", None),
            discovered("make-build", None),
            discovered("make-lint", None),
        ];
        let phase = run(
            &mechs,
            vec![
                (
                    "make-test",
                    Ok(exec_result("make-test", Outcome::Passed, None, None)),
                ),
                (
                    "make-build",
                    Ok(exec_result(
                        "make-build",
                        Outcome::Timeout,
                        None,
                        Some("slow"),
                    )),
                ),
                (
                    "make-lint",
                    Ok(exec_result(
                        "make-lint",
                        Outcome::NotRun,
                        None,
                        Some("watcher"),
                    )),
                ),
            ],
        );
        assert_eq!(phase.executed.len(), 3);
        assert_eq!(
            phase.limitations,
            vec![
                "make make-build: slow".to_string(),
                "make make-lint: watcher".to_string()
            ]
        );
        assert!(infra_blocked_areas(
            &phase.executed_results,
            &[unknown_gate(GateName::Build), unknown_gate(GateName::Tests),]
        )
        .is_empty());
    }

    // ----- infra_blocked_areas -----

    fn both_gates() -> Vec<Gate> {
        vec![unknown_gate(GateName::Build), unknown_gate(GateName::Tests)]
    }

    /// The three admitted outcomes each produce an entry for the backed gate.
    #[test]
    fn infra_blocked_areas_builder_covers_error_failed_and_blocked() {
        for outcome in [Outcome::Error, Outcome::Failed, Outcome::Blocked] {
            let results = [pair(
                "make-test",
                outcome,
                Some(Class::InfraLevel),
                Some("detail"),
            )];
            let areas = infra_blocked_areas(&results, &both_gates());
            assert_eq!(
                areas,
                vec![BlockedArea {
                    area: "Tests".to_string(),
                    blocking_condition: "detail".to_string(),
                    execution_result_ref: IdRef("make-test".to_string()),
                }],
                "{outcome:?}"
            );
        }
    }

    /// Only INFRA_LEVEL and only ERROR/FAILED/BLOCKED produce an entry.
    #[test]
    fn infra_blocked_areas_builder_ignores_other_shapes() {
        let results = [
            pair(
                "make-test",
                Outcome::Error,
                Some(Class::Unclassified),
                Some("d"),
            ),
            pair("make-test", Outcome::Error, None, Some("d")),
            pair(
                "make-test",
                Outcome::Error,
                Some(Class::AppLevel),
                Some("d"),
            ),
            pair("make-build", Outcome::Timeout, None, Some("d")),
            pair("make-build", Outcome::NotRun, None, Some("d")),
            pair("make-build", Outcome::Passed, None, None),
            // INFRA with an outcome outside {ERROR, FAILED, BLOCKED}.
            pair(
                "make-build",
                Outcome::Timeout,
                Some(Class::InfraLevel),
                Some("d"),
            ),
        ];
        assert!(infra_blocked_areas(&results, &both_gates()).is_empty());
    }

    /// A gate that is absent or N/A yields no entry and no invented area.
    #[test]
    fn infra_blocked_areas_builder_requires_present_applicable_gate() {
        let results = [pair(
            "make-test",
            Outcome::Error,
            Some(Class::InfraLevel),
            Some("d"),
        )];

        let absent = [unknown_gate(GateName::Build)];
        assert!(infra_blocked_areas(&results, &absent).is_empty());
        assert!(infra_blocked_areas(&results, &[]).is_empty());

        let mut na = unknown_gate(GateName::Tests);
        na.state = GateState::NotApplicable;
        na.requirement_level = RequirementLevel::NotApplicable;
        assert!(infra_blocked_areas(&results, &[na]).is_empty());
    }

    /// A mechanism that backs no gate (lint, variants, unknown ids) yields none.
    #[test]
    fn infra_blocked_areas_builder_skips_mechanism_without_backing_gate() {
        let results = [
            pair(
                "make-lint",
                Outcome::Error,
                Some(Class::InfraLevel),
                Some("d"),
            ),
            pair(
                "npm-test:coverage",
                Outcome::Error,
                Some(Class::InfraLevel),
                Some("d"),
            ),
        ];
        assert!(infra_blocked_areas(&results, &both_gates()).is_empty());
    }

    /// A mechanism whose `run_existing` returned `Err` has no result, so it
    /// contributes no entry (there is no `execution_result_ref` to point at).
    #[test]
    fn infra_blocked_areas_builder_ignores_mechanism_with_err() {
        let mechs = [discovered("make-test", None)];
        let phase = run(
            &mechs,
            vec![(
                "make-test",
                Err(CapabilityError::Failed("boom".to_string())),
            )],
        );
        assert!(infra_blocked_areas(&phase.executed_results, &both_gates()).is_empty());
    }

    /// Entries follow execution order and refs resolve to executed rows.
    #[test]
    fn infra_blocked_areas_builder_orders_entries_and_keeps_references() {
        let mechs = [
            discovered("make-test", None),
            discovered("make-build", None),
            discovered("npm-test", None),
        ];
        let phase = run(
            &mechs,
            vec![
                (
                    "make-test",
                    Ok(exec_result(
                        "make-test",
                        Outcome::Error,
                        Some(Class::InfraLevel),
                        Some("t1"),
                    )),
                ),
                (
                    "make-build",
                    Ok(exec_result(
                        "make-build",
                        Outcome::Error,
                        Some(Class::InfraLevel),
                        Some("b1"),
                    )),
                ),
                (
                    "npm-test",
                    Ok(exec_result(
                        "npm-test",
                        Outcome::Error,
                        Some(Class::InfraLevel),
                        Some("t2"),
                    )),
                ),
            ],
        );
        let gates = gates_from_phase(&levels(), &phase);
        let areas = infra_blocked_areas(&phase.executed_results, &gates);
        let summary: Vec<(&str, &str, &str)> = areas
            .iter()
            .map(|a| {
                (
                    a.area.as_str(),
                    a.blocking_condition.as_str(),
                    a.execution_result_ref.0.as_str(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Tests", "t1", "make-test"),
                ("Build", "b1", "make-build"),
                ("Tests", "t2", "npm-test"),
            ]
        );
        for a in &areas {
            assert!(
                phase
                    .executed
                    .iter()
                    .any(|r| r.command_id == a.execution_result_ref),
                "dangling reference {a:?}"
            );
            assert!(!a.blocking_condition.is_empty());
        }
        // The blocked gates are UNKNOWN/INFRA, decided by CORE.
        for name in [GateName::Build, GateName::Tests] {
            let g = gate_of(&gates, name);
            assert_eq!(
                (g.state, g.cause),
                (GateState::Unknown, Some(GateCause::Infra))
            );
        }
    }

    /// With no detail, the fixed fallback text is used; the area is not invented.
    #[test]
    fn infra_blocked_areas_builder_uses_fixed_fallback_without_detail() {
        let results = [pair(
            "make-build",
            Outcome::Failed,
            Some(Class::InfraLevel),
            None,
        )];
        let areas = infra_blocked_areas(&results, &both_gates());
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].area, "Build");
        assert_eq!(areas[0].blocking_condition, INFRA_BLOCKED_FALLBACK);
        assert!(!areas[0].blocking_condition.is_empty());
    }

    // ----- build_coverage -----

    /// R5.3: a gate in `blocked_areas` is not skipped; each gate is in exactly
    /// one of the three lists; the rest keeps its previous classification.
    #[test]
    fn gate_in_blocked_areas_is_not_in_skipped() {
        let mut pass = unknown_gate(GateName::Build);
        pass.state = GateState::Pass;
        let gates = vec![
            pass,
            unknown_gate(GateName::Tests),
            unknown_gate(GateName::Security),
        ];
        let blocked = vec![BlockedArea {
            area: "Tests".to_string(),
            blocking_condition: "detail".to_string(),
            execution_result_ref: IdRef("make-test".to_string()),
        }];
        let cov = build_coverage(&gates, Vec::new(), Vec::new(), blocked.clone());

        assert_eq!(cov.blocked_areas, blocked);
        let skipped: Vec<&str> = cov.skipped_areas.iter().map(|s| s.area.as_str()).collect();
        assert_eq!(skipped, vec!["Security"]);
        assert_eq!(
            cov.skipped_areas[0].reason,
            "audit interpretation (P2-P11) not yet performed"
        );
        let audited: Vec<&str> = cov.audited_areas.iter().map(|a| a.area.as_str()).collect();
        assert_eq!(audited, vec!["Build"]);

        for g in &gates {
            let name = gate_wire(&g.name);
            let n = cov.audited_areas.iter().filter(|a| a.area == name).count()
                + cov.skipped_areas.iter().filter(|a| a.area == name).count()
                + cov.blocked_areas.iter().filter(|a| a.area == name).count();
            assert_eq!(n, 1, "gate {name} must be in exactly one list");
        }
    }

    /// Without blocked areas the coverage is what it was before this slice.
    #[test]
    fn build_coverage_without_blocked_areas_is_unchanged() {
        let gates = vec![unknown_gate(GateName::Tests)];
        let cov = build_coverage(&gates, Vec::new(), Vec::new(), Vec::new());
        assert!(cov.blocked_areas.is_empty());
        assert_eq!(cov.skipped_areas.len(), 1);
        assert_eq!(cov.skipped_areas[0].area, "Tests");
    }

    // ----- validate_report with a non-empty `blocked_areas` -----

    /// The golden with its `Tests` entry moved from `audited_areas` to
    /// `blocked_areas` (referencing the golden's executed row) passes VR8 and the
    /// other wired rules.
    #[test]
    fn validate_report_accepts_non_empty_blocked_areas() {
        let mut value = golden_value();
        let audited = value["coverage"]["audited_areas"]
            .as_array_mut()
            .expect("audited_areas");
        audited.retain(|a| a["area"].as_str() != Some("Tests"));
        value["coverage"]["blocked_areas"] = json!([{
            "area": "Tests",
            "blocking_condition": "infrastructure impediment",
            "execution_result_ref": "cmd-tests"
        }]);

        let report: AuditReport =
            serde_json::from_value(value).expect("report with blocked_areas deserializes");
        assert_eq!(report.coverage.blocked_areas.len(), 1);
        assert!(report
            .coverage
            .executed_mechanisms
            .iter()
            .any(|m| m.command_id == report.coverage.blocked_areas[0].execution_result_ref));
        let violations = validate_report(&report);
        assert!(violations.is_empty(), "got: {violations:?}");
    }

    /// The same shape built by `build_coverage` from stub-driven results.
    #[test]
    fn coverage_built_from_infra_results_has_intact_blocked_areas() {
        let mechs = [discovered("cargo-test", None)];
        let phase = run(
            &mechs,
            vec![(
                "cargo-test",
                Ok(exec_result(
                    "cargo-test",
                    Outcome::Error,
                    Some(Class::InfraLevel),
                    Some("no launcher"),
                )),
            )],
        );
        let gates = gates_from_phase(&levels(), &phase);
        let blocked = infra_blocked_areas(&phase.executed_results, &gates);
        assert_eq!(blocked.len(), 1);

        let cov = build_coverage(&gates, phase.executed, phase.limitations, blocked);
        // Shape and referential integrity only: the golden's gates and
        // mappings are unrelated to this stub run, so it is not validated.
        let report: AuditReport = serde_json::from_value(golden_value_with(&cov))
            .expect("coverage with blocked_areas deserializes into a report");
        assert_eq!(report.coverage.blocked_areas[0].area, "Tests");
        assert_eq!(
            report.coverage.blocked_areas[0].execution_result_ref.0,
            "cargo-test"
        );
        assert!(
            report
                .coverage
                .executed_mechanisms
                .iter()
                .any(|m| m.command_id == report.coverage.blocked_areas[0].execution_result_ref),
            "referential integrity"
        );
        assert!(
            !report
                .coverage
                .skipped_areas
                .iter()
                .any(|s| s.area == "Tests"),
            "a blocked gate is not skipped"
        );
    }

    fn golden_value_with(cov: &fira_core::coverage::CoverageStatement) -> Value {
        let mut value = golden_value();
        value["coverage"] = serde_json::to_value(cov).expect("coverage serializes");
        value
    }
}
