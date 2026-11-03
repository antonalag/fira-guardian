# FIRA Guardian — Validation Evidence Tiers

> Authoritative record of **what kind of evidence** backs FIRA Guardian's
> behavior and **what each kind can and cannot establish**. Honest by
> construction: it does not fabricate reports or claim validation that did not
> occur. Frozen contract: `1.0-frozen+corr1-4` (unchanged). Roadmap index:
> `docs/roadmap.md`.

Confidence tags: **[observed]** read from the cited file; **[inferred]** reasoned;
**[open]** not established.

---

## 1. The four evidence tiers

| Tier | What it is | CAN establish | CANNOT establish |
|---|---|---|---|
| **1. Synthetic fixtures** | Hand-built temp project trees (e.g. a `Makefile`/`package.json` written by a test) | Controlled behavior of discovery / execution / gate derivation / classification for a specific constructed case | That a real project behaves this way |
| **2. Unit / integration tests** | In-crate `#[test]` + `tests/` suites | Component correctness, determinism, the VR gauntlet, boundary/error handling | End-to-end real-audit viability |
| **3. Discovery-level validation** | Running discovery against a **real** checkout and comparing the resulting mechanism registry | That discovery parses a real manifest correctly (e.g. the Makefile correction) | That execution, gate evaluation, or the verdict complete |
| **4. Complete real-project audit** | A full audit run to a persisted `report.json`/`report.md` against a real checkout | End-to-end viability + report provenance **for that project/profile only** | Generalization to other profiles / build tools / CI providers |

**No universal-support promise.** None of these tiers implies universal support
for Makefile syntax, build tools, CI providers, or project layouts. Discovery is
intentionally small and extended only by adding structural parsers
(`crates/fira-runtime/src/discovery.rs` module doc) [observed].

---

## 2. Current evidence by capability (summary)

- **Discovery** (Cargo / npm scripts / Makefile): Tier 1 + Tier 2 [observed]; one
  Tier 3 observation (below). CI/Dockerfile discovery is **not implemented**
  (`docs/roadmap.md §3.1`).
- **Bounded execution / timeout / non-terminating** (Task 14): Tier 1 + Tier 2 —
  `crates/fira-runtime/tests/execution_bounds.rs` (real interrupt, stdin
  isolation, partial-output capture, durable-agents-shaped termination)
  [observed].
- **Build/Tests gate derivation** (execution-gate-eval-slice): Tier 1 + Tier 2
  [observed].
- **Persistence / WS-1** (Task 11): Tier 2 — 15 persistence tests observed green
  [observed].
- **Fencing property** (Option A): Tier 1 + Tier 2; capped at **UNVERIFIED** by
  design (a passing suite is not proof the property was exercised) [observed].
- **Verdict engine, confidence rubric, VR validator, re-audit lifecycle**: Tier 2
  (unit-tested) but **not wired into the real CLI path** [observed] — so there is
  **no** Tier 4 evidence for them end-to-end.
- **Complete real-project audit (Tier 4): none to date** [observed/inferred].

---

## 3. Honest status of the named real-project validations

### 3.1 `durable-agents`

- **Tier 1 (fixture shapes):** several tests construct `durable-agents`-shaped
  trees — e.g. `crates/fira-runtime/tests/execution_bounds.rs`
  (`durable_agents_shaped_project_terminates`) and a Tests/Build e2e in
  `crates/fira-adapters/tests/cli.rs` [observed].
- **Design anchor:** the 300s execution bound is documented as anchored to an
  observation that `durable-agents`' real suite finishes in ~25s
  (`crates/fira-runtime/src/execution.rs` doc comment) [observed].
- **What exists:** fixture-shape + unit/integration evidence and a design anchor.
- **What does NOT exist:** a **committed authentic audit report** for
  `durable-agents`. None is in the repository (only
  `schemas/examples/valid/s11-audit-report.json`, a schema example) [observed].
- **Honest claim:** `durable-agents` informed fixtures and the execution bound;
  it has **not** been validated by a committed Tier-4 full audit.

### 3.2 `k8s-ai-driven-event-pipeline`

- **Tier 3 (discovery-level only):** the Makefile variable-assignment correction
  (filtering `:=` / `::=` / `:::=` so they are not emitted as phantom targets) is
  validated by
  `crates/fira-runtime/tests/execution.rs::makefile_variable_assignments_are_not_targets`
  [observed]. The `surface-failed-mechanisms-slice` design notes its Makefile
  fixture "mirrors the k8s audit shape" [observed].
- **The attempted full audit did NOT complete.** It must **not** be represented
  as a successful end-to-end audit.
- **What does NOT exist:** any committed artifact of a k8s audit. There is
  **zero** occurrence of "k8s" / "kubernetes" / "event-pipeline" anywhere in the
  repository [observed].
- **Honest claim:** the k8s work is a **Tier-3 discovery-level registry
  comparison** that validated the Makefile correction; the full audit is
  **incomplete** and is recorded as a limitation, not a result.

---

## 4. Bounded cross-project validation plan (future; not executed in Phase 0)

A bounded matrix for when the pipeline matures (dependent on the cross-cutting
slices in `docs/roadmap.md §4`). Each cell is labeled with the tier it can
achieve; **Tier-4 cells are aspirational** until failure classification + finding
synthesis land.

- **Profiles:** `library` (Rust/Cargo), `cli_tool` (npm `bin`); and —
  only after the review-gated signal detectors land — `web_service`,
  `batch_pipeline`, `stateful_distributed` (until then: explicit `--profile`).
- **Discovery configs:** Cargo-only; package.json-scripts-only; Makefile-only;
  mixed; (future) CI/Dockerfile.
- **Execution configs:** all-pass; one FAILED backing; one TIMEOUT; one NOT_RUN
  (watcher); capability-gap.

**Discipline:** synthetic fixtures and discovery-level comparisons may be
committed; a Tier-4 report must be produced by an **actual rerun** against a real
checkout and must never be fabricated or back-filled as if original. Incomplete
audits are documented as limitations, not successes.

---

## 5. What Phase 0 did and did not do

- **Did:** record the tiers and the honest status above by inspection.
- **Did not:** create any fixture, audit report, or script; run any mechanism or
  test; change any source/test/schema/contract/`CONTRACT_VERSION`.
