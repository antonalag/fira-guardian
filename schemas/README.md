# schemas/

Concrete, machine-validatable schema definitions (JSON Schema, Draft 2020-12) for
**S1–S12**, plus the example corpus that exercises the schemas and the validation
rules **VR1–VR13**. Produced in **Task 2 — Schemas**, derived verbatim from the
source of truth:
[`../docs/contract/schema-formalization.md`](../docs/contract/schema-formalization.md)
under the Frozen MVP Contract (`1.0-frozen+corr1-4`). Task 2 introduces no new
rule, invariant, or semantic — it is a translation of approved design text into
checkable artifacts.

## Dialect and identity

- **Dialect:** JSON Schema Draft 2020-12 (`$schema` set explicitly on every file).
  Chosen for `if/then/else`, `$defs`, and `$ref`; validated offline (TRANSPORT-1:
  local process only).
- **`$id`:** URN-style, embedding the contract version, e.g.
  `urn:fira:schema:1.0-frozen+corr1-4:s07-finding`. Cross-references use the full
  `$id` plus a JSON pointer (`#/$defs/...`). The version segment must equal
  `fira-core::CONTRACT_VERSION` (asserted by `schema_parity.rs`).
- **`additionalProperties: false`** on every object definition — exact field
  parity with the schema doc (no silent extra fields).

## Files

Shared building blocks:
- `_defs/common.schema.json` — `GateName` (11 gates, §20), `CapabilitySet`,
  facet / outcome / severity / maturity / lifecycle enums, id patterns.

Schemas S1–S12 (one file each):
- `s01-audit-request.schema.json` — S1 AuditRequest
- `s02-audit-profile.schema.json` — S2 AuditProfile
- `s03-epistemic-state.schema.json` — S3 EpistemicState
- `s04-evidence-support-mapping.schema.json` — S4 EvidenceRef + SupportMapping
- `s05-execution-result.schema.json` — S5 ExecutionResult
- `s06-requirement-family.schema.json` — S6 DeclaredClaim / InferredInvariant /
  ReleaseRequirement / SecurityProperty
- `s07-finding.schema.json` — S7 Finding + FailureScenario + Confidence
- `s08-gate.schema.json` — S8 Gate
- `s09-coverage-statement.schema.json` — S9 CoverageStatement
- `s10-assessment-decision.schema.json` — S10 TechnicalAssessment + HumanDecision
- `s11-audit-report.schema.json` — S11 AuditReport (canonical)
- `s12-verification-mechanism.schema.json` — S12 VerificationMechanism

Example corpus:
- `examples/valid/` — one passing instance per S1–S12 (schemas with co-equal
  `$defs`, i.e. S4/S6/S10, get one file per definition), plus the golden
  composite `s11-audit-report.json` that also passes every validator VR.
- `examples/invalid/` — one fixture per rejectable rule, each otherwise-valid and
  failing on exactly the target rule:
  - Structural (fail at the JSON Schema): `vr03-command-missing-output.json`,
    `vr04-na-mismatch.json`, `vr06-fira-accepted.json`,
    `vr10-blocker-bad-source.json` (out-of-enum `requirement_source`).
  - Validator (fail at the VR validator; each is a full S11 report that is
    schema-valid): `vr01-no-sufficient-mapping.json`,
    `vr02-recovery-missing-anchor.json`, `vr08-gate-without-coverage.json`,
    `vr09-safe-language.json`, `vr11-static-only-verified.json`,
    `vr12-synthesized-command.json`, `vr13-write-under-project-root.json`.
  - VR5/VR7 surface only: `vr05-surface-insufficient-finding.json` asserts only
    what is checkable now (the CONF-2/VR1 overlap), not a confidence recompute.

The Rust VR validator lives in `crates/fira-core/src/validation/` and is
exercised by `crates/fira-core/tests/{schema_parity,schema_examples,vr_validator}.rs`.

## VR → enforcement-site map

Each of VR1–VR13 is enforced structurally (in the JSON Schema), semantically (in
the `fira-core` VR validator), both, or is surface-only (recompute deferred).

| VR   | Rule (from schema doc)                                  | Structural (JSON Schema) | Validator (semantic) | Notes |
|------|---------------------------------------------------------|:------------------------:|:--------------------:|-------|
| VR1  | support_mappings nonempty; PASS/VERIFIED ≥1 SUFFICIENT  | partial (`minItems:1`)   | yes (SUFFICIENT link)| both |
| VR2  | conc/dur/recovery ⇒ failure_scenario.anchor non-null    | partial (anchor shape)   | yes (category couple)| both |
| VR3  | captured_output_ref iff type=COMMAND                    | **yes** (s04 if/then/else)| no                  | structural only |
| VR4  | state=N/A iff requirement_level=not_applicable          | **yes** (s08 allOf)      | no                   | structural only |
| VR5  | recompute confidence & compare                          | surface only             | **deferred**         | Task 6/15; surface + fixtures + hook only |
| VR6  | FIRA HumanDecision ∈ {NOT_APPLICABLE,PENDING}           | **yes** (HumanDecisionFira)| no                 | structural only |
| VR7  | recompute technical_assessment & compare                | surface only             | **deferred**         | Task 7/15; surface + fixtures + hook only |
| VR8  | every gate in ≥1 coverage entry                         | no                       | **yes**              | cross-document |
| VR9  | no "correct/safe" adjudication language                 | no                       | **yes**              | lexical scan (fixed token list) |
| VR10 | BLOCKER ⇒ source ∈ {declared,release} or code-internal  | **yes** (enum guard)     | no                   | **structural only** (see below) |
| VR11 | VERIFIED ⇒ ≥1 execution-based facet                     | partial (facet present)  | **yes**              | correction 1 |
| VR12 | command_id ⇒ declared_by_project=true mechanism         | no                       | **yes**              | correction 3, FK |
| VR13 | no write/report path under project_root                 | no                       | **yes**              | correction 4, path |

### VR10 — structural only in Task 2 (recorded decision)

The frozen contract requires `release_impact=BLOCKER` to have `requirement_source
∈ {declared_claim, release_requirement}` **or a code-internal correctness
invariant violation**. `InferredInvariant` is defined as "code-internal
correctness only", so the escape clause maps onto the `inferred_invariant` enum
value, and the canonical `AuditReport` Finding carries **no field** that
distinguishes a qualifying "code-internal correctness invariant violation" from
any other `inferred_invariant`-sourced finding. That semantic distinction is
**not representable in the current model**, so it is not implemented as an extra
validator check in Task 2 (doing so would require a new field/rule — out of
scope). VR10 is therefore enforced structurally only: the S7 schema constrains
`requirement_source` and `release_impact` to their enums. This is an enforcement
decision limited to the information the contract already exposes; it is not a
contract change.

### VR5 / VR7 — surface + fixtures only

VR5 (confidence) and VR7 (verdict) are recompute-and-compare acceptance criteria
for later tasks (6, 7, 15). Task 2 ships only their **schema surface** — S7
`confidence` / `confidence_derivation`; S10 `computed_by_rule`; S11
`determinism_inputs_hash` — plus a stable plug-in hook
(`fira_core::validation::RecomputeHook`, with the Task 2 no-op `NoRecompute`) and
fixtures asserting only what is checkable now. Task 2 implements **no** rubric or
verdict computation.
