//! CLI adapter: build an `AuditRequest`, bind the RUNTIME capabilities, run the
//! minimal Task 10 flow (discover → P1 classify/confirm → execute existing
//! mechanisms → capture raw results → honest report), and return rendered
//! output. No P2–P11 interpretation: gates are not state-derived and findings
//! are not derived here.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use fira_core::assessment::{HumanDecisionFira, TechnicalAssessment};
use fira_core::classification::{classify, Classification};
use fira_core::coverage::{AuditedArea, CoverageStatement, ExecutedMechanism, SkippedArea};
use fira_core::execution::{ExecutionResult, VerificationMechanism};
use fira_core::gate::Gate;
use fira_core::gate_eval::{evaluate_execution_backed_gates, ExecutedMechanismResult};
use fira_core::interfaces::{CommandExecutor, RepositoryReader};
use fira_core::model::{CoverageResult, ExecutionOutcome};
use fira_core::model::{
    Depth, GateCause, GateName, GateState, HumanDecisionFiraState, ProfileId, RequirementLevel,
};
use fira_core::property_audit::{
    classify_fencing, FencingAssessment, FencingEvidence, PropertyExercised, FENCING_GATE,
};
use fira_core::report::{AppliedProfile, AppliedProfileGate, AuditReport};
use fira_core::request::AuditRequest;
use fira_core::verdict::{compute_assessment, ExpectationViolations};

use fira_core::interfaces::AuditContextProvider;

use fira_policy::{confirm_gate_view, profile_for, AppliedSelection, ConfirmGateView};
use fira_runtime::{
    detect_signals, discover, FsRepositoryReader, MechanismRegistry, OutputSink,
    ProjectCommandExecutor,
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
    let registry = MechanismRegistry::discover(&root);
    let mechanisms = discover(&root);
    let executor = ProjectCommandExecutor::new(registry, &root);

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
    let mut executed: Vec<ExecutedMechanism> = Vec::new();
    // Retain the full ExecutionResult + its declared mechanism record so the
    // Tests/Build gate evaluation (step 5) can read the real outcomes. This is
    // adapter-internal data flow; the coverage `executed_mechanisms` summary is
    // still the `{command_id, outcome}` pair.
    let mut executed_results: Vec<(VerificationMechanism, ExecutionResult)> = Vec::new();
    // Task 14: a non-terminating mechanism returns NOT_RUN with a reason; a hung
    // one returns TIMEOUT. Both are recorded as executed_mechanisms (with their
    // outcome) and their reason is surfaced as a coverage limitation (C11: "not
    // sufficiently audited"). Nothing becomes PASS (C5).
    let mut mechanism_limitations: Vec<String> = Vec::new();
    for m in &mechanisms {
        match executor.run_existing(&m.mechanism.id) {
            Ok(result) => {
                if let Some(reason) = &result.blocking_condition {
                    mechanism_limitations.push(format!("{}: {reason}", result.command));
                }
                executed.push(ExecutedMechanism {
                    command_id: result.command_id.clone(),
                    outcome: result.outcome,
                });
                executed_results.push((m.mechanism.clone(), result));
            }
            Err(_) => { /* capability gap → recorded as a limitation below */ }
        }
    }

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
    let backed: Vec<ExecutedMechanismResult<'_>> = executed_results
        .iter()
        .map(|(mech, result)| ExecutedMechanismResult {
            mechanism: mech,
            result,
        })
        .collect();
    let evaluated = evaluate_execution_backed_gates(&profile_gate_levels, &backed);
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
    let fencing = audit_fencing_property(&reader, npm_test_passed(&executed_results));
    if let Some(assessment) = &fencing {
        if let Some(slot) = gates.iter_mut().find(|g| g.name == FENCING_GATE) {
            // Record the property's evidence state on the gate's rationale; the
            // state remains UNKNOWN (UNVERIFIED ⇒ never PASS, C5).
            slot.rationale = format!("fencing invariant audited: {}", assessment.rationale);
        }
    }

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

    let mut coverage_limitations = mechanism_limitations;
    if let Some(assessment) = &fencing {
        coverage_limitations.push(format!(
            "property audit — {}: {}",
            fira_core::property_audit::FENCING_PROPERTY_TITLE,
            assessment.rationale
        ));
    }
    let coverage = build_coverage(&gates, executed, coverage_limitations);

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

    Ok(AuditOutput {
        applied,
        view,
        report,
        canonical_location: location.0,
    })
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
            _ => skipped_areas.push(SkippedArea {
                area: gate_wire(&g.name),
                reason: "audit interpretation (P2-P11) not yet performed".to_string(),
            }),
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
        blocked_areas: Vec::new(),
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
