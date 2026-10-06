# FIRA Guardian

An independent, evidence-based **Release Readiness Auditor** that runs as an
agent/subagent alongside coding agents. FIRA Guardian independently inspects a
project and its existing verification mechanisms, establishes its own evidence,
and emits a traceable technical assessment of whether the project is ready to
release. The coding agent is never the source of truth; developer-agent
assertions carry zero evidentiary weight until independently verified.

> Status: **MVP under construction.** Frozen contract version `1.0-frozen+corr1-4`.

## Layer map

| Crate | Layer | Responsibility |
|-------|-------|----------------|
| `fira-core`         | CORE         | Platform-independent audit domain, engines, abstract capability interfaces. No sibling-crate / host / OS deps. |
| `fira-policy`       | POLICY       | Versioned data: 5 built-in audit profiles, severity model, confidence-rubric parameters, release-gate rules. |
| `fira-runtime`      | RUNTIME      | Capability enforcement `{READ, EXECUTE_EXISTING}`, VerificationMechanism registry (EXEC-1), ExecutionResult, external-workspace persistence (WS-1). |
| `fira-presentation` | PRESENTATION | Renders JSON/Markdown from the canonical `AuditReport`. |
| `fira-adapters`     | ADAPTER      | Host bindings (CLI first, Kiro second) over the Core interfaces. |
| `fira-cli`          | ADAPTER      | First adapter entrypoint (`fira-guardian` binary). |

Dependency direction (enforced by tests): `core` → nothing internal; `policy` →
`core`; `runtime` → `core`,`policy`; `presentation` → `core`; `adapters` →
all lower layers; `cli` → `adapters`.

## Source of truth

- Frozen contract: [`docs/contract/frozen-mvp-contract.md`](docs/contract/frozen-mvp-contract.md)
- Schema formalization (S1–S12, VR1–VR13): [`docs/contract/schema-formalization.md`](docs/contract/schema-formalization.md)
- Language decision: [`docs/adr/ADR-001-implementation-language.md`](docs/adr/ADR-001-implementation-language.md)
- Invariant enforcement strategy: [`docs/adr/ADR-002-invariant-enforcement-strategy.md`](docs/adr/ADR-002-invariant-enforcement-strategy.md)
- Contract version marker: [`CONTRACT_VERSION`](CONTRACT_VERSION)

## Security boundary (why it matters)

FIRA's independence is partly a **runtime property**, not only a prompt property.
It reads the project and executes only project-declared verification mechanisms;
it never writes to the audited project tree. Its own reports persist to an
external audit workspace (`/fira-workspace/<audit-id>/`), a separate concern from
project capabilities.

## Build

```
cargo build
cargo test
```
