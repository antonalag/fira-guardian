# ADR-002: How the Capability and Write-Isolation Invariants Are Enforced

**Status:** ACCEPTED

## Decision

FIRA's project-tree capability and write-isolation invariants — CAP-1,
EXEC-1, WS-1, and P-1 (defined in `docs/contract/frozen-mvp-contract.md`) — are
enforced in layers, with a declared primary/secondary ordering:

1. **Primary — architectural dependency boundaries.** CORE (`fira-core`) and
   POLICY (`fira-policy`) cannot write to the project tree or spawn processes
   because they do not depend on the crate that owns those capabilities
   (`fira-runtime`). The capability simply is not reachable from those layers.
2. **Primary — runtime capability ownership.** `fira-runtime` is the only layer
   that performs project execution and workspace persistence. It owns capability
   enforcement, the VerificationMechanism registry (EXEC-1), and
   external-workspace-only persistence (WS-1 / P-1 / VR13).
3. **Secondary — source-scan tripwire.** A test scans CORE/POLICY sources for
   forbidden `std::fs` write / `std::process` calls. This is defense-in-depth,
   not the mechanism of record.

A second decision travels with this one: **project-tree write capability and
audit-report persistence are independent axes.** Persisting FIRA's own report
to the external audit workspace must never be implemented by granting a
project-tree write capability. The project-tree capability set stays
`{READ, EXECUTE_EXISTING}` with no write member, regardless of report-saving
needs.

This strategy is frozen for the MVP. The invariants themselves are owned by the
frozen contract; this ADR records only *how* they are enforced and *why* that
layering was chosen.

## Context

The contract states the invariants but is deliberately silent on enforcement
mechanism. Without a recorded strategy, each later task that touches capabilities
or persistence (notably the runtime and adapter work) would re-decide where
enforcement lives, and the tempting shortcut — "just add a write capability so
the report can be saved" — would quietly collapse the two axes and defeat WS-1.

Two failure modes motivated an explicit ordering:

- Treating the source-scan as the real guarantee. A grep is easy to weaken or
  work around and says nothing about transitive dependencies; relying on it would
  give false confidence.
- Conflating persistence with project write. The report must land somewhere, and
  the path of least resistance is a generic write capability over the project
  tree — exactly what WS-1 forbids.

## Consequences

**Positive**
- The strongest guarantee is structural: a layer physically lacks the capability
  it must not have, provable from the dependency graph rather than from reviewer
  vigilance.
- The independent-axes rule gives later tasks a clear constraint: persistence is
  a runtime concern targeting the external workspace, never a project-tree write.
- If the source-scan tripwire ever fires, the correct fix is to move the offending
  capability to its owning layer, not to relax the scan.

**Negative / accepted trade-offs**
- Report persistence cannot be a convenience write from CORE; it must route
  through a runtime capability, which is more plumbing than an in-place write.
- The dependency direction must be preserved precisely; a stray dependency from
  CORE/POLICY onto the runtime would silently erode the primary guarantee, so the
  direction is itself guarded by test.

## Scope

- This ADR does not define or modify any contract invariant, field, or enum.
- It does not add a capability type or a write variant.
- Runtime enforcement (mechanism 2) is implemented in the runtime/adapter tasks,
  not here; this ADR only fixes the strategy those tasks must follow.

## References

- `docs/contract/frozen-mvp-contract.md` — CAP-1, EXEC-1, WS-1, P-1 (§2), the
  capability model (§17), and persistence (§19).
- `docs/contract/schema-formalization.md` — VR12 (command FK) and VR13
  (write-path containment), the validator-level checks that back EXEC-1/WS-1.
- `docs/adr/ADR-001-implementation-language.md` — the language choice that makes
  the structural boundary and future sandboxing tractable.
