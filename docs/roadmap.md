# FIRA Guardian — Roadmap Index

> **Index only.** This document is a navigational, evidence-based summary. The
> **authoritative** tracking for each unit remains its own spec folder under
> `.kiro/specs/` (`requirements.md` / `design.md` / `tasks.md`). This index does
> **not** replace them, does **not** invent task IDs, and does **not** renumber
> or rewrite history. Frozen contract: `1.0-frozen+corr1-4` (unchanged).

Status vocabulary (evidence-per-acceptance-criterion standard):
- **Verified** — evidence supports every acceptance criterion.
- **Partial** — delivered, but at least one criterion is unmet or overstated.
- **Core-complete, integration-pending** — library logic delivered + tested, but
  not wired into the real audit path.
- **Unverified** — code merged and tests exist, but a per-AC pass has not been
  performed this phase (not a claim of incompleteness; a claim about evidence).

Confidence tags: **[observed]** read from the cited file; **[inferred]** reasoned;
**[open]** not established.

---

## 1. Numbered tasks

| Task (authoritative spec) | Evidence-based status | Notes / evidence |
|---|---|---|
| task-01-foundation | Unverified | Workspace guards; not re-inspected per-AC this phase [open] |
| task-02-schemas | Unverified (strongly supported) | S1–S12 schemas + VR validator present [observed]; full corpus/VR-parity pass pending |
| task-04-core-domain | Unverified | Serde domain types present [observed]; AC pass pending |
| task-05-evidence-execution-interfaces | Unverified | `ExecutionResult`/evidence/S12 types present [observed]; AC pass pending |
| task-06-confidence-rubric | Partial | VR5 rubric + recompute hook present [observed]; **not wired into the CLI** |
| task-07-verdict-engine | Partial (strongly supported) | §11 engine present and **called** in CLI [observed]; runs over empty findings; VR7 recompute has no implementor |
| task-08-policy-profiles | Unverified | Five profiles present [observed]; AC pass pending |
| task-09-system-classification | Partial | CORE classifier faithful to design §4 [observed]; consuming `indicates_*` signals **not detected** by RUNTIME → advanced classes `--profile`-only (see §3.2) |
| task-10-runtime-cli | **Partial** | Discovery scope overstated (no CI/Dockerfile); advanced-class signal detection absent (see §3.1, §3.2) |
| task-11-persistence | Unverified (strongly supported) | WS-1 external-workspace sink + 15 persistence tests observed green [observed] |
| task-12-reaudit-lifecycle | **Core-complete, integration-pending** | CORE `reconcile`/`decide` delivered + unit-tested [observed]; §2.2 flow wiring / AC-9 **not** demonstrated — `reconcile` not called by `fira-cli`/`fira-adapters`; adapter provider `prior: None` [observed]. Historical anomaly: lifecycle commit message says "Task 11" though the spec dir is `task-12-reaudit-lifecycle` — **noted, not rewritten** |
| task-13-kiro-adapter | Unverified | Kiro adapter merged [observed]; AC pass pending |
| task-14-execution-safety-bounds | Partial (strongly supported) | Bounded exec / 300s / timeout / non-terminating + tests [observed]; TIMEOUT coverage representation gap (see §3.3) |

## 2. Unnumbered slices (delivered)

| Slice (authoritative spec) | Evidence-based status | Notes |
|---|---|---|
| execution-gate-eval-slice | Partial (strongly supported) | Execution-backed Build/Tests gates [observed]; scoped to two gates by design |
| fencing-invariant-property-slice | Partial (strongly supported) | Single-property audit, **Option A UNVERIFIED ceiling** [observed]; suite-pass is not property-exercised |
| surface-failed-mechanisms-slice | Partial (strongly supported) | FAILED mechanisms surfaced as negative-evidence limitations; no gate/finding change by design [observed] |
| phase0-reconciliation-slice | This slice | Documentation-only reconciliation (this index + `docs/validation/README.md`) |

> No historical checkbox is changed by Phase 0. Checking boxes in a task's own
> `tasks.md` requires a focused per-AC pass first (the §Status standard above).

---

## 3. Reconciliation notes (Phase 0)

### 3.1 Discovery scope (Decision 1)
Delivered discovery parses **Cargo.toml, package.json scripts, Makefile targets**
only (`crates/fira-runtime/src/discovery.rs`) [observed]. The Task 10 spec text
additionally lists "CI/Dockerfile"; those parsers are **not implemented** and are
**deferred** (tracked in §4). Not a contract violation: the §6/S12 `source` enum
lists allowed sources, not a delivery mandate [inferred]. The frozen contract and
the Task 10 spec are **not** edited.

### 3.2 Advanced-profile auto-classification gap (Decision 2)
`detect_signals` never sets `indicates_service` / `indicates_batch_job` /
`indicates_stateful_distribution` [observed], so auto-classification resolves only
to `library`, `cli_tool`, or `Undetermined`; the three advanced classes are
reachable **only via `--profile`**. Human confirm + `--profile` authority is
intact [observed]. Conservative detectors are a **review-gated proposal**
(slice `design.md §2.1`), not implemented.

### 3.3 TIMEOUT coverage (Decision 4 — Option X)
Contract §13 / S9 schema support `blocked_areas{area, blocking_condition,
execution_result_ref}`; the Task 14 spec expects TIMEOUT there; the delivered
`build_coverage` leaves `blocked_areas` empty and routes TIMEOUT + NOT_RUN into
`known_limitations` [observed]. This is a **spec-vs-code gap, not a contract
violation** (empty `blocked_areas` is schema-valid; semantics preserved)
[inferred]. **Phase 0 = Option X (document only).** Option Y (populate
`blocked_areas` for TIMEOUT) is optional future work (§4). No change made.

---

## 4. Cross-cutting work (proposed; dependency-ordered; NOT implemented)

Represented as **new unnumbered slices** (precedent: the delivered slices). **No
task IDs invented.** Names are proposals; nothing below is created or implemented
in Phase 0.

1. **report-provenance-slice** — real `created_at`; `determinism_inputs_hash`;
   accurate TIMEOUT/NOT_RUN/BLOCKED coverage (incl. the §3.3 Option-Y question).
   *Prereq:* none. *Acceptance:* identical hash across unchanged re-audits.
2. **vr-enforcement-wiring-slice** — wire `run_vrs` + the VR5 confidence recompute
   into the CLI; record that VR7 verdict recompute has **no implementor** yet.
   *Prereq:* 1 (provenance fields exist for the validated report).
3. **discovery-and-signals-slice** — CI/Dockerfile discovery (§3.1) + the
   review-gated `indicates_*` detectors (§3.2). *Prereq:* heuristic sign-off.
4. **failure-classification-slice** — APP/INFRA `ExecutionResult.classification`
   (§6); the prerequisite between an observed FAILED/TIMEOUT and a gate
   FAIL/finding. *Prereq:* 1–2.
5. **finding-synthesis-slice** — evidence→finding (P8) + justified gate evaluation
   beyond Build/Tests; fencing stays Option A/UNVERIFIED. *Prereq:* 4.
6. **reaudit-integration-slice** — wire the delivered CORE `reconcile` into the
   CLI (§task-12). **Separate from finding synthesis** (Decision 6). *Prereq:* 5
   (findings must exist) + prior-report load/persist.
7. **narrative-divergence-slice** (deferred) — P7 narrative ingestion + divergence,
   after core evaluation is stable. *Prereq:* 5 (immutable blind observations).

Dependency spine [inferred]: `1 → 2 → 3 → 4 → 5 → 6 → 7`; items 1/2/3 are largely
mutually independent; item 4 is the key blocker for 5–6.

> Each cross-cutting slice, and the §3.2 signal heuristics, require explicit human
> approval before any code. Phase 0 only records them.

---

## 5. Invariants preserved (unchanged by Phase 0)

Frozen contract + `CONTRACT_VERSION`; schemas; READ + EXECUTE_EXISTING capability
boundary; blind-first; human risk acceptance; deterministic verdict semantics;
the 300s execution bound; the Option A fencing UNVERIFIED ceiling; deferral of
narrative/P7. See `docs/contract/frozen-mvp-contract.md` and
`docs/validation/README.md`.
