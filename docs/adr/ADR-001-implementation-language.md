# ADR-001: FIRA Guardian Implementation Language

**Status:** ACCEPTED

## Decision

Implement FIRA Guardian **Core, Policy, Runtime, and the CLI adapter in Rust**.
Adapters for TS-native hosts (Kiro, Claude Code, Codex, Cursor) integrate with the
Rust Core over a process/IPC boundary — the CLI/subagent invocation contract
(TRANSPORT-1) — not by in-process embedding.

Frozen for the MVP. Do not reopen unless an explicit architectural blocker makes
implementation impossible.

## Context

FIRA Guardian's credibility depends on (a) a deterministic Core (CONF-1, C10,
recompute-and-compare rules VR5/VR7), (b) compiler-enforceable domain invariants
for the epistemic / gate / verdict / lifecycle state machines, and (c) a security
boundary enforced by the runtime itself — not by prompts (CAP-1, EXEC-1, WS-1,
VR11–VR13). Process execution, filesystem isolation, command allowlisting, and
future sandboxing were weighted heavily.

## Alternatives considered

Rust, TypeScript, Python — evaluated across 17 weighted criteria.

## Decision matrix (weighted totals)

| Language   | Score |
|------------|-------|
| Rust       | 326   |
| TypeScript | 271   |
| Python     | 235   |

Weights prioritized: runtime-enforced security boundary, determinism, invariant
enforcement, product-vision fit, and self-contained distribution.

## Consequences

**Positive**
- Algebraic enums + exhaustive `match` make illegal audit states
  unrepresentable (epistemic/gate/verdict/lifecycle).
- Deterministic engines by construction (no GC pauses, no hidden coercions).
- `std::process::Command` with explicit arg vectors (no shell) maps directly to
  EXEC-1 command allowlisting.
- Clearest path to future sandboxing (landlock/seccomp/cgroups/WASI) for
  CAP-1/WS-1.
- Single static binary → self-contained auditor, easy distribution to users
  without a language runtime.

**Negative / accepted trade-offs**
- Slower initial velocity than TS/Python.
- JSON Schema runtime tooling more manual than TS (serde + a schema crate).
- TS-native host adapters (Kiro included) run over a process/IPC boundary rather
  than in-process — consistent with the frozen delegation model; a purely
  implementation-level implication.

## Rejected alternatives

- **TypeScript** — best schema/adapter ergonomics and in-process host
  integration, but structurally weakest on the runtime-enforceable security
  boundary (the most heavily weighted property) and weaker determinism
  guarantees.
- **Python** — fastest prototyping, but weakest determinism/invariant
  enforcement, no language-level isolation, worst self-contained distribution,
  GIL-limited parallel audits.

## MVP implications (implementation-level only; no contract changes)

- Tasks 1–12 (Core, Policy, Runtime, CLI, persistence, re-audit) are Rust.
- Task 13 (Kiro adapter) is a subprocess/IPC integration invoking the Rust
  auditor, marshalling AuditRequest/AuditResponse and the confirmation-gate UX
  through Kiro — consistent with TRANSPORT-1 and the four capability interfaces.
- No change to Frozen MVP Contract semantics.
