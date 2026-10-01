# FIRA Guardian — Frozen MVP Contract

`CONTRACT STATUS: FROZEN for MVP` · `CONTRACT VERSION: 1.0-frozen+corr1-4`

This document is the authoritative transcription of the approved Frozen MVP
Contract, including corrections 1–4. It is a **source-of-truth document**, not
code. Later tasks must conform to it; if a task appears to require changing it,
STOP and report the conflict instead of editing the contract.

---

## 0. Product identity

FIRA Guardian is an independent, evidence-based Release Readiness Auditor that
runs as an agent/subagent alongside coding agents. Core is platform-independent.
CLI is the first adapter, Kiro the second. The coding agent is never the source
of truth; developer-agent assertions have zero evidentiary weight until
independently verified.

---

## 1. Layer ownership

- **CORE** (platform-independent): epistemic model, claim/requirement/invariant
  model, evidence model, execution-result model, VerificationMechanism model,
  finding model, gate model, technical assessment, deterministic verdict engine,
  coverage model, re-audit lifecycle, trust model, security boundary, capability
  interfaces (abstract), confidence rubric, invocation-contract shapes.
- **POLICY** (versioned data): the 5 built-in audit profiles, severity model,
  confidence-rubric parameters, release-gate rules.
- **RUNTIME**: capability enforcement `{READ, EXECUTE_EXISTING}`,
  VerificationMechanism registry enforcement, production of ExecutionResult,
  blind-input gating.
- **ADAPTER**: CLI (MVP first), Kiro (MVP second); transport, host capability
  binding, persistence location, human-confirmation UX.
- **PRESENTATION**: Markdown/JSON rendering from the canonical machine report.

---

## 2. Hard invariants

- **C1** No project modification.
- **C2** No generated tests/scripts/experiments/temp artifacts.
- **C3** No invented business requirements.
- **C4** FIRA never accepts risk.
- **C5** Lack of evidence never becomes PASS.
- **C6** Static code reading yields at most `{IMPLEMENTED}`; static evidence may
  ground a FINDING but never an epistemic VERIFIED. *(correction 1)*
- **C7** Every finding/PASS cites valid+sufficient evidence.
- **C8** Unanchored failure scenario = hypothesis, not finding.
- **C9** Blind-first: narrative deferred to P7.
- **C10** Verdict is deterministic.
- **C11** Report distinguishes "no issue found in audited area" from "not
  sufficiently audited".
- **C12** Prior report is historical evidence, not truth.
- **CONF-1** Confidence is a deterministic function of the evidence model; no
  model-overridable confidence field; ambiguous evidence resolves to the lower
  level.
- **CONF-2** LOW != INSUFFICIENT. Every published FINDING (any confidence level)
  has ≥1 SUFFICIENT SupportMapping. Insufficient evidence stays HYPOTHESIS
  (findings) / UNKNOWN (gates), never a published FINDING. *(correction 2)*
- **TRUST-1** Developer-agent assertions have zero evidentiary weight until
  independently verified.
- **CAP-1** MVP capabilities = `{READ, EXECUTE_EXISTING}` against the project
  tree; zero write capability anywhere inside the audited project tree; NETWORK
  denied (no profile opt-in in MVP).
- **EXEC-1** FIRA may execute an existing verification mechanism discovered in
  the project; it may not synthesize a new verification command. Runtime-
  enforced: executor rejects any `command_id` not in the discovered
  VerificationMechanism registry. *(correction 3)*
- **WS-1** FIRA has zero write capability to the audited project tree. Report
  persistence targets an external audit workspace
  (`/fira-workspace/<audit-id>/`), not the project tree. Adapter-local
  convenience locations (e.g., `.fira/`) are adapter/presentation concerns
  layered on top and must still honor WS-1 for Core/runtime. *(correction 4)*
- **P-1** Report persistence writes only to the external audit workspace, never
  into the audited project tree.
- **PROF-1** Only the 5 built-in versioned profiles exist in MVP; humans may
  select/reject/reclassify/override-N/A but cannot redefine gate semantics or
  create profiles at audit time.
- **TRANSPORT-1** MVP transport is local CLI/process; no HTTP/MCP/server. Core is
  transport-independent.

---

## 3. Epistemic model

`EpistemicState = { facets: Set<{SPECIFIED, IMPLEMENTED, TESTED, OBSERVED}>,
conclusion: VERIFIED | UNVERIFIED }`. Facets orthogonal (not a ladder).

- **SPECIFIED**: declared in an artifact.
- **IMPLEMENTED**: static code evidence.
- **TESTED**: existing relevant test body inspected.
- **OBSERVED**: execution + observation of an existing artifact.
- **VERIFIED**: sufficient execution/observation evidence.
- **E1** VERIFIED requires ≥1 execution-based facet (OBSERVED, or TESTED whose
  test was actually executed with PASSED exercising the property). **NO
  static-only and NO multi-evidence path to VERIFIED.** *(correction 1)*
- **E5** VERIFIED for failure/recovery/concurrency/durability requires the
  failure case itself exercised.
- FORBIDDEN: deriving VERIFIED from SPECIFIED / IMPLEMENTED / TESTED-not-executed.

---

## 4. Claim / Requirement / Invariant model

- **DeclaredClaim** (project-asserted, can_fail_release).
- **InferredInvariant** (auditor-derived, code-internal correctness only, must
  carry `derivation_evidence`, `inferred=true`, never presented as a business
  requirement).
- **ReleaseRequirement** (from profile, can_fail_release).
- **SecurityProperty** (category overlay: source in {declared, inferred,
  profile}; can_fail_release inherits source).

---

## 5. Evidence model

`EvidenceRef{ ref_id, type in {CODE,TEST,CONFIG,COMMAND,DOC}, locator, valid,
captured_output_ref? required iff type=COMMAND }` (validity)
+ `SupportMapping{ claim, evidence_refs≥1, relevant_span, sufficiency in
{SUFFICIENT,INSUFFICIENT}, unverified_remainder required if not SUFFICIENT }`
(sufficiency). PASS/VERIFIED require ≥1 SUFFICIENT mapping.

---

## 6. Execution + VerificationMechanism model

`VerificationMechanism{ id, source in {MAVEN,GRADLE,NPM,MAKE,CI,DOCKER,SCRIPT,
OTHER}, source_locator (file:line of declaration), command, declared_by_project:
true }`. Discovered during inspection from manifests/CI/Docker/Make; never
synthesized. *(correction 3)*

`ExecutionResult{ command_id (FK to VerificationMechanism.id), command, outcome
in {PASSED,FAILED,NOT_RUN,BLOCKED,TIMEOUT,ERROR}, classification in
{APP_LEVEL,INFRA_LEVEL,UNCLASSIFIED}?, blocking_condition?, captured_output,
exercised_behaviors[] }`.

Outcome → epistemic/gate mapping: PASSED adds OBSERVED/TESTED facet scoped only
to `exercised_behaviors`; FAILED(APP_LEVEL) = app negative evidence → FAIL
candidate; FAILED(INFRA_LEVEL) = like BLOCKED → UNKNOWN; NOT_RUN → UNKNOWN;
BLOCKED → UNKNOWN unless the blocking condition is itself an operational blocker;
TIMEOUT → UNKNOWN unless property is liveness/perf then APP negative; ERROR
classify first (INFRA → BLOCKED-like, APP → FAILED-like, UNCLASSIFIED → UNKNOWN).
"Command executed" never auto-implies "behavior verified".

`CommandExecutor` exposes `run_existing(command_id)` ONLY; no arbitrary shell
input. EXEC-1 enforced at runtime.

---

## 7. Gate model

States `{PASS, FAIL, PARTIAL, UNKNOWN, N/A}` via decision table: positive
sufficient evidence no gaps → PASS; positive sufficient with gaps → PARTIAL;
APP_LEVEL negative evidence → FAIL; no positive/no negative → UNKNOWN; profile
not_applicable → N/A. `requirement_level in {required, recommended,
not_applicable}` from profile. N/A only via profile. PARTIAL requires some
positive sufficient evidence (else UNKNOWN). UNKNOWN != FAIL (FAIL has negative
evidence).

---

## 8. Confidence rubric (deterministic, derived; symmetric)

`confidence = f(epistemic conclusion, evidence facets, evidence sufficiency,
execution outcomes, property type, failure-case coverage)`.

- **HIGH**: direct execution-based evidence, SUFFICIENT mapping, appropriate
  coverage; for failure/recovery/concurrency/durability the failure scenario
  exercised.
- **MEDIUM**: SUFFICIENT but indirect, incomplete execution coverage, or adjacent
  to the exact property.
- **LOW**: SUFFICIENT evidence to publish, but weak/indirect with meaningful
  ambiguity/residual uncertainty.

Ambiguity resolves downward among the three; never below LOW into publishable.
INSUFFICIENT evidence exits the finding path (HYPOTHESIS/UNKNOWN). No override
field. *(correction 2: LOW ≠ INSUFFICIENT)*

---

## 9. Finding model + taxonomy

Maturities: OBSERVATION, HYPOTHESIS (not publishable), FINDING (anchored +
sufficient), RISK. Finding fields: `id` (RR-NNN stable), `maturity`, `title`,
`severity {CRITICAL,HIGH,MEDIUM,LOW,INFO}`, `confidence` (derived) +
`confidence_derivation`, `category`, `requirement_source {declared_claim,
inferred_invariant, release_requirement}`, `security_property{source}?`,
`gates_affected[]`, `epistemic_state`, `support_mappings[]≥1`,
`failure_scenario{ anchor (file:line, required for concurrency/durability/
recovery), trigger, consequence }?`, `impact`, `release_impact {BLOCKER, RISK,
NON_BLOCKING}`, `recommended_remediation`, `verification_criteria`,
`lifecycle_status {OPEN,FIX_CLAIMED,VERIFIED_FIXED,REOPENED,WONT_FIX}`,
`regression`. `release_impact=BLOCKER` requires `requirement_source in
{declared_claim, release_requirement}` or a code-internal correctness invariant
violation.

---

## 10. Risk acceptance model

`TechnicalAssessment {READY, READY_WITH_RISKS, NOT_READY}` (FIRA-owned) ×
`HumanDecision {NOT_APPLICABLE, PENDING, ACCEPTED, REJECTED}` (external). FIRA
sets only NOT_APPLICABLE | PENDING; never ACCEPTED. Composition:
READY+NOT_APPLICABLE → RELEASE_OK; READY_WITH_RISKS+PENDING → AWAITING_DECISION;
READY_WITH_RISKS+ACCEPTED → READY_WITH_ACCEPTED_RISKS; READY_WITH_RISKS+REJECTED
→ BLOCKED_BY_DECISION; NOT_READY+any → NOT_READY. FIRA never emits
READY_WITH_ACCEPTED_RISKS.

---

## 11. Deterministic verdict algorithm

Step0 normalize gates via decision table + apply minimum_evidence_expectations.
Step1 blocking set: B1 CRITICAL+{HIGH,MEDIUM}+BLOCKER → block (CRITICAL+LOW does
not auto-block but forbids PASS on its gates); B2 HIGH+BLOCKER on a required gate
→ block; B3 required gate FAIL → block; B4 required gate UNKNOWN → block; B5
required gate PARTIAL blocks only if it violates a minimum_evidence_expectation,
else RISK. Step2 risks: recommended gates in {UNKNOWN,FAIL,PARTIAL}; findings
release_impact=RISK; required PARTIAL not blocking; infra-caused UNKNOWN on
required gate still blocks (annotate cause=INFRA, remediation="provide
environment"), on recommended → RISK. Step3 first-match: BLOCKING nonempty →
NOT_READY; elif RISKS nonempty → READY_WITH_RISKS + HumanDecision=PENDING; else →
READY + HumanDecision=NOT_APPLICABLE. Step4: N/A gates never enter blocking/risks;
multiple MEDIUMs never aggregate by count; REOPENED evaluated as OPEN; WONT_FIX
retains severity and still enters blocking/risks; VERIFIED_FIXED excluded.
Post-audit human acceptance never changes TechnicalAssessment, only the composed
release state. Determinism: no rule depends on model judgment.

---

## 12. Blind-first workflow (P0–P11)

P0 scope+boundary; P1 system classification (structural manifests only) → HUMAN
CONFIRM GATE (shows class + resulting gate matrix + focus areas + N/A
justifications); P2 blind understanding (allowed: source code, directory
structure, structural manifests package.json/pom.xml/build.gradle/Cargo.toml/
go.mod/Dockerfile/CI config/lockfiles, code-level type defs/schemas, existing
test code; forbidden: README, design docs, specs, ADRs, developer conclusions,
architecture narratives, PR descriptions, prose-rationale comments, prior audit
narrative); P3 guarantees extraction from code; P4 risk targeting (driven by
profile focus_areas + typical_failure_modes); P5 evidence collection (read +
execute existing mechanisms via registry); P6 epistemic classification; P7
developer narrative contrast (narrative first accessible here) producing
immutable `DivergenceRecord{ blind_observation_ref, narrative_claim_ref,
divergence_type in {DOC_OVERCLAIMS,DOC_UNDERCLAIMS,DOC_CONTRADICTS_IMPL,
DOC_MATCHES}, resulting_finding? }`; P8 findings + evidence validation; P9 gate
evaluation per profile; P10 deterministic verdict; P11 dual report + coverage.
Blind observations from P2–P6 are immutable; P7 annotates/diverges, never
overwrites.

---

## 13. Coverage model

`CoverageStatement{ audited_areas[{area, depth SHALLOW|STANDARD|DEEP, result
NO_ISSUE_FOUND|ISSUES_FOUND}], skipped_areas[{area,reason}], blocked_areas[{area,
blocking_condition, execution_result_ref}], executed_mechanisms[{command_id,
outcome}], known_limitations[], assumptions[], unanchored_hypotheses[] }`.
NO_ISSUE_FOUND always carries depth and never implies proof of correctness. Every
gate maps to ≥1 coverage entry. No "correct/safe" language anywhere.

---

## 14. Re-audit lifecycle

OPEN → FIX_CLAIMED (claim, no evidence, verdict-equivalent to OPEN) →
VERIFIED_FIXED (requires new valid+SUFFICIENT evidence meeting
verification_criteria, execution-based where the original property required
execution) | REOPENED (issue still present or claimed fix unverifiable). any →
REOPENED on regression (previously VERIFIED_FIXED recurs; flag regression).
OPEN/REOPENED → WONT_FIX (recorded; retains severity; still blocks). Stable IDs
across rounds; new problems get new IDs; VERIFIED_FIXED re-checked each round. A
prior report is historical evidence, never current truth; never blindly trust
"fixed".

---

## 15. Trust model

TrustedInputs (repo path, release_target, declared_requirements,
requested_profile, prior_audit_report, execution_boundary) accepted as
scope/context, not truth. UntrustedClaims (any assertion about implementation
state) recorded as DeclaredClaim to verify, zero evidentiary weight, never an
EvidenceRef. AuditorEvidence (source inspection, existing tests body-inspected,
execution results, build results, existing verification-command output, config
inspection, reproducible observations) is the only source of PASS/VERIFIED.

---

## 16. Security boundary

IN SCOPE (can be a FINDING): source inspection, config inspection, existing
security tests (body-inspected/executed), existing security verification
mechanisms, dependency manifest inspection. OUT OF SCOPE (report
OUT_OF_SCOPE/UNVERIFIED, never inferred as a vuln): crafting/sending exploit
payloads, dynamic injection/SSRF/XSS probing, fuzzing, auth-bypass attempts
against a running instance, any active exploitation. A statically-evident issue
can be a FINDING with UNVERIFIED epistemic conclusion (execution not required to
publish the finding, but required for VERIFIED).

---

## 17. Capability model (interfaces)

- `RepositoryReader{ read_file(path,range?), list_structure(path,depth?),
  resolve_ref(locator)->bool }`.
- `CommandExecutor{ run_existing(command_id)->ExecutionResult,
  capabilities()->Set<Capability>, discover_mechanisms()->[VerificationMechanism]
  }`.
- `EvidenceCollector{ capture(execution_result)->EvidenceRef, attach(finding|gate,
  support_mapping) }`.
- `AuditContextProvider{ get_request()->AuditRequest,
  get_prior_report()->AuditReport?, persist_report(AuditReport)->location }`.

Core calls only these four; capability gaps become coverage limitations, not
crashes.

**Project capabilities vs audit-workspace persistence.** Project-tree
capabilities are `{READ, EXECUTE_EXISTING}` with no `WRITE_PROJECT`,
`CREATE_FILE`, or `DELETE_FILE` and NETWORK denied. Audit-workspace persistence
(saving FIRA's own report to `/fira-workspace/<audit-id>/`, WS-1) is a **separate
concern** and does not grant, imply, or require any write capability over the
audited project tree.

---

## 18. Invocation contract

`AuditRequest{ audit_id, project_root, release_target,
execution_boundary(CapabilitySet, MVP={READ,EXECUTE_EXISTING}), declared_claims[]?,
audit_profile(ProfileId)?, prior_audit_report_ref?, requested_depth? }` →
`AuditResponse{ technical_assessment, human_decision, findings, gates, coverage,
machine_report, human_report }`. MVP transport = local CLI.

---

## 19. Persistence (MVP)

External audit workspace `/fira-workspace/<audit-id>/{report.json, report.md}`,
versioned, local, inspectable, re-audit input. No database. Zero writes under
`project_root` (WS-1 / P-1 / VR13). Adapter `.fira/` convenience deferred.

---

## 20. Profiles (MVP, versioned POLICY data)

`library`, `cli_tool`, `web_service`, `stateful_distributed`, `batch_pipeline`.
Each: `profile_id`, `version`, `description`, `gates[{gate, requirement_level,
na_justification?}]`, `focus_areas[]`, `typical_failure_modes[]`,
`minimum_evidence_expectations[{gate, expectation}]`. Selecting a profile must
change required/recommended/N/A gates AND focus_areas/typical_failure_modes (a
labels-only profile is invalid). N/A legal only via not_applicable.
minimum_evidence_expectations define what a PASS needs for that class.

Gates: Build, Tests, Requirements, Correctness, FailureRecovery,
PersistenceDurability, Concurrency, Security, Observability, Documentation,
OperationalReadiness.

Reference matrix (R=required, r=recommended, -=n/a):
- library: Build R, Tests R, Requirements R, Correctness R, Documentation R,
  Security r, Concurrency r, FailureRecovery r, PersistenceDurability -,
  Observability -, OperationalReadiness -.
- stateful_distributed: Build R, Tests R, Requirements R, Correctness R,
  FailureRecovery R, PersistenceDurability R, Concurrency R, Security R,
  OperationalReadiness R, Observability r, Documentation r.
- cli_tool, web_service, batch_pipeline defined analogously and kept small.

---

## Corrections applied to this frozen version (1–4)

1. **VERIFIED requires execution-based evidence** — removed the multi-evidence
   path; static evidence can ground a FINDING but never epistemic VERIFIED
   (see §2 C6, §3 E1/E5, VR11).
2. **LOW ≠ INSUFFICIENT** — LOW is still a published finding backed by ≥1
   SUFFICIENT mapping; insufficient evidence stays HYPOTHESIS/UNKNOWN
   (see §2 CONF-2, §8, VR5).
3. **VerificationMechanism model / EXEC-1** — FIRA executes only discovered,
   project-declared mechanisms by `command_id`; no synthesized commands; runtime-
   enforced (see §2 EXEC-1, §6, §17, VR12).
4. **External audit workspace / WS-1** — zero write capability to the audited
   project tree; persistence targets an external workspace; capability and
   persistence are separate axes (see §2 WS-1/P-1, §17, §19, VR13).
