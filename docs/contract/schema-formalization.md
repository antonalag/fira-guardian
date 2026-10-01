# FIRA Guardian — Schema Formalization

`CONTRACT VERSION: 1.0-frozen+corr1-4`

Design-level schema definitions (S1–S12) and validation rules (VR1–VR13) derived
from the Frozen MVP Contract. Format-neutral; concrete machine-validatable
definitions (e.g., JSON Schema) are produced in **Task 2**. This is a
source-of-truth design artifact, not code.

---

## Schemas (S1–S12)

### S1. AuditRequest
```
AuditRequest
  audit_id: string                       # generated if absent
  project_root: path                     # required
  release_target: string                 # required
  execution_boundary: CapabilitySet      # required; MVP={READ,EXECUTE_EXISTING}
  declared_claims: [DeclaredClaim]?       # → UntrustedClaims
  audit_profile: ProfileId?               # if absent → Core infers, human confirms
  prior_audit_report_ref: path?           # re-audit
  requested_depth: SHALLOW|STANDARD|DEEP?
```

### S2. AuditProfile (POLICY)
```
AuditProfile
  profile_id: enum{library,cli_tool,web_service,stateful_distributed,batch_pipeline}
  version: semver
  description: string
  gates: [ { gate: GateName, requirement_level: required|recommended|not_applicable, na_justification?: string } ]
  focus_areas: [string]
  typical_failure_modes: [string]
  minimum_evidence_expectations: [ { gate: GateName, expectation: string } ]
```

### S3. EpistemicState
```
EpistemicState
  facets: Set<enum{SPECIFIED,IMPLEMENTED,TESTED,OBSERVED}>
  conclusion: enum{VERIFIED,UNVERIFIED}
```
Constraint: conclusion=VERIFIED requires ≥1 execution-based facet (OBSERVED, or
executed-PASSED TESTED). No static-only VERIFIED (E1/VR11).

### S4. EvidenceRef + SupportMapping
```
EvidenceRef
  ref_id: string
  type: enum{CODE,TEST,CONFIG,COMMAND,DOC}
  locator: string                         # file:line-range | test_id | config:key | command_id | doc:section
  valid: bool
  captured_output_ref: command_id?        # required iff type=COMMAND (VR3)

SupportMapping
  claim: string
  evidence_refs: [ref_id]                 # ≥1
  relevant_span: string
  sufficiency: enum{SUFFICIENT,INSUFFICIENT}
  unverified_remainder: string            # required if sufficiency≠SUFFICIENT
```

### S5. ExecutionResult
```
ExecutionResult
  command_id: string                      # FK → VerificationMechanism.id (VR12)
  command: string
  outcome: enum{PASSED,FAILED,NOT_RUN,BLOCKED,TIMEOUT,ERROR}
  classification: enum{APP_LEVEL,INFRA_LEVEL,UNCLASSIFIED}?
  blocking_condition: string?
  captured_output: text
  exercised_behaviors: [string]
```

### S6. Requirement family
```
DeclaredClaim     { id, statement, source_ref, security_relevant: bool }
InferredInvariant { id, statement, derivation_evidence: [ref_id], inferred: true, code_internal_only: true }
ReleaseRequirement{ id, statement, from_profile: ProfileId, gate: GateName }
SecurityProperty  { id, statement, source: enum{declared,inferred,profile}, underlying_ref }
```

### S7. Finding + FailureScenario + Confidence
```
Finding
  id: string                              # RR-NNN stable
  maturity: enum{FINDING,RISK}            # OBSERVATION/HYPOTHESIS live in coverage appendix
  title: string
  severity: enum{CRITICAL,HIGH,MEDIUM,LOW,INFO}
  confidence: enum{HIGH,MEDIUM,LOW}       # DERIVED, not model-set (CONF-1)
  confidence_derivation: string           # rubric branch that produced it (audit trail)
  category: string
  requirement_source: enum{declared_claim,inferred_invariant,release_requirement}
  security_property: { source: declared|inferred|profile }?
  gates_affected: [GateName]
  epistemic_state: EpistemicState
  support_mappings: [SupportMapping]      # ≥1, and ≥1 SUFFICIENT for any level (CONF-2/VR5)
  failure_scenario: { anchor: file:line-range, trigger: string, consequence: string }?  # required for concurrency/durability/recovery (VR2)
  impact: string
  release_impact: enum{BLOCKER,RISK,NON_BLOCKING}
  recommended_remediation: string
  verification_criteria: string
  lifecycle_status: enum{OPEN,FIX_CLAIMED,VERIFIED_FIXED,REOPENED,WONT_FIX}
  regression: bool
```

### S8. Gate
```
Gate
  name: GateName
  requirement_level: enum{required,recommended,not_applicable}
  state: enum{PASS,FAIL,PARTIAL,UNKNOWN,N/A}
  rationale: string
  cause: enum{APP,INFRA,NONE}?
  support_mappings: [SupportMapping]
  supporting_findings: [finding_id]
```

### S9. CoverageStatement
```
CoverageStatement
  audited_areas: [ { area, depth: SHALLOW|STANDARD|DEEP, result: NO_ISSUE_FOUND|ISSUES_FOUND } ]
  skipped_areas: [ { area, reason } ]
  blocked_areas: [ { area, blocking_condition, execution_result_ref } ]
  executed_mechanisms: [ { command_id, outcome } ]
  known_limitations: [string]
  assumptions: [string]
  unanchored_hypotheses: [string]
```

### S10. TechnicalAssessment + HumanDecision
```
TechnicalAssessment
  result: enum{READY,READY_WITH_RISKS,NOT_READY}
  blocking_findings: [finding_id]
  blocking_gates: [GateName]
  risk_findings: [finding_id]
  risk_gates: [GateName]
  computed_by_rule: string
HumanDecision
  state: enum{NOT_APPLICABLE,PENDING,ACCEPTED,REJECTED}   # FIRA sets only NOT_APPLICABLE|PENDING (VR6)
```

### S11. AuditReport (canonical)
```
AuditReport
  schema_version: semver
  audit_id: string
  created_at: timestamp
  request: AuditRequest
  applied_profile: { profile_id, version, gates, na_overrides: [ {gate, by:human} ] }
  system_model_summary: string
  verification_mechanisms: [VerificationMechanism]   # discovered registry
  divergence_records: [ { blind_observation_ref, narrative_claim_ref, divergence_type, resulting_finding? } ]
  findings: [Finding]
  gates: [Gate]
  coverage: CoverageStatement
  technical_assessment: TechnicalAssessment
  human_decision: HumanDecision
  prior_report_ref: path?
  determinism_inputs_hash: string         # hash of structured state fed to verdict engine
```

### S12. VerificationMechanism
```
VerificationMechanism
  id: string
  source: enum{MAVEN,GRADLE,NPM,MAKE,CI,DOCKER,SCRIPT,OTHER}
  source_locator: string                  # file:line of declaration
  command: string
  declared_by_project: true               # always true; never synthesized (EXEC-1/VR12)
```

---

## Validation rules (VR1–VR13)

- **VR1** Every finding `support_mappings` nonempty; any PASS/VERIFIED has ≥1
  SUFFICIENT mapping.
- **VR2** concurrency/durability/recovery finding ⇒ `failure_scenario.anchor`
  non-null.
- **VR3** `EvidenceRef.captured_output_ref` present iff `type=COMMAND`.
- **VR4** `Gate.state=N/A` iff `requirement_level=not_applicable`.
- **VR5** Recompute confidence and compare: all levels require ≥1 SUFFICIENT
  mapping; INSUFFICIENT evidence must not be a FINDING; HIGH requires an
  execution-based facet and, for failure-class properties, failure-case coverage.
  *(non-negotiable acceptance criterion for Tasks 6, 15)*
- **VR6** FIRA-produced `HumanDecision` in {NOT_APPLICABLE, PENDING}.
- **VR7** Recompute `technical_assessment` via the verdict engine and compare.
  *(non-negotiable acceptance criterion for Tasks 7, 15)*
- **VR8** Every gate appears in ≥1 coverage entry.
- **VR9** No "correct/safe" adjudication language in rationale/summary.
- **VR10** `release_impact=BLOCKER` ⇒ `requirement_source in {declared_claim,
  release_requirement}` or a code-internal correctness invariant.
- **VR11** Any `EpistemicState.conclusion=VERIFIED` references ≥1 execution-based
  facet (OBSERVED or executed-PASSED TESTED); static-only VERIFIED rejected.
  *(correction 1)*
- **VR12** Every `ExecutionResult.command_id` and executed reference resolves to
  a VerificationMechanism with `declared_by_project=true`; synthesized commands
  rejected. *(correction 3)*
- **VR13** No report path or runtime write targets inside `project_root`;
  persistence under the external workspace only. *(correction 4)*

VR5 and VR7 are recompute-and-compare rules: a validator recomputes confidence
and the verdict from the structured state and rejects any report where the model
deviated. This is what enforces "intelligence in the contract, not the prompt."

---

## Schema ↔ VR ↔ Task ownership map

| Schema | Owning task | Key VRs |
|--------|-------------|---------|
| S1 AuditRequest            | 4 / 9        | —              |
| S2 AuditProfile            | 8            | VR4            |
| S3 EpistemicState          | 4            | VR11           |
| S4 Evidence/SupportMapping | 5            | VR1, VR3       |
| S5 ExecutionResult         | 5            | VR12           |
| S6 Requirement family      | 4            | VR10           |
| S7 Finding                 | 4 / 6        | VR1,VR2,VR5,VR10|
| S8 Gate                    | 4            | VR4, VR8       |
| S9 CoverageStatement       | 4            | VR8, VR9       |
| S10 Assessment/Decision    | 4 / 7        | VR6, VR7       |
| S11 AuditReport            | 4            | VR9, VR13      |
| S12 VerificationMechanism  | 5            | VR12           |
