//! Validation-rule (VR) enforcement — semantic / cross-reference portion.
//!
//! Task 2 splits VR1–VR13 enforcement between the JSON Schema files (structural
//! rules VR3, VR4, VR6, VR10) and this module (the non-structural portion of
//! VR1, VR2, VR8, VR9, VR11, VR12, VR13). VR5 and VR7 are recompute-and-compare
//! acceptance criteria for later tasks; Task 2 provides only their schema
//! surface, fixtures, and a stable plug-in hook (see [`RecomputeHook`]).
//!
//! ## VR10 is structural-only in Task 2
//!
//! VR10 (`release_impact=BLOCKER` ⇒ `requirement_source ∈ {declared_claim,
//! release_requirement}` or a code-internal correctness invariant violation) is
//! enforced **structurally**: the S7 schema constrains `requirement_source` to
//! its three-value enum and `release_impact` to its enum. The frozen contract
//! defines `InferredInvariant` as "code-internal correctness only", so the VR10
//! escape clause maps onto the `inferred_invariant` enum value; the canonical
//! `AuditReport` Finding carries **no field** that distinguishes a qualifying
//! "code-internal correctness invariant violation" from any other
//! `inferred_invariant`-sourced finding. That semantic distinction is therefore
//! not representable in the current model and is **not** implemented as an extra
//! validator check in Task 2 (doing so would require a new field/rule — out of
//! scope). VR10 accordingly has no variant in [`Vr`] and no rule module here.
//!
//! ## Layer ownership (why this lives in CORE)
//!
//! Per the Frozen MVP Contract (Layer ownership), the epistemic / evidence /
//! finding / gate / coverage model and the deterministic checks over it belong
//! to CORE. Every function here is a **pure** computation over an
//! already-parsed [`serde_json::Value`] plus a small [`ValidationContext`]. This
//! module performs no filesystem access, no process spawning, and no network I/O
//! — the Task 1 `no-fs/no-proc` tripwire (`core_and_policy_sources_have_no_fs_write_or_process`)
//! must keep passing. Host-bound inputs (the real `project_root`, the discovered
//! mechanism registry) are supplied by the caller through `ValidationContext`;
//! binding them to a real host is a runtime/adapter concern in later tasks.
//!
//! Task 2 introduces **no** new rule, invariant, or semantic decision. Each
//! check below is a mechanical consequence of `schema-formalization.md`
//! (VR1–VR13) under the frozen contract; VR11/VR12/VR13 remain the mechanical
//! carry-overs of corrections 1/3/4.

use serde_json::Value;

mod rules;

/// The validation rules whose non-structural portion this validator enforces.
///
/// Structural rules VR3, VR4, VR6 are enforced entirely in the JSON Schema and
/// are intentionally absent here. VR5 and VR7 are recompute-and-compare rules
/// deferred to Tasks 6/7/15 and are therefore not variants of this enum in
/// Task 2 (only their surface + hook exist — see [`RecomputeHook`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vr {
    /// VR1 — every finding has nonempty `support_mappings`; any PASS/VERIFIED
    /// carries ≥1 SUFFICIENT mapping. (`minItems` is structural; the SUFFICIENT
    /// requirement is semantic.)
    Vr1,
    /// VR2 — concurrency/durability/recovery finding ⇒ `failure_scenario.anchor`
    /// non-null.
    Vr2,
    /// VR5 — recompute confidence and compare. This is the label for the
    /// recompute-and-compare rule whose **deterministic** portion is implemented
    /// by the Task 6 confidence rubric (see [`crate::confidence`]). It is **not**
    /// run by [`run_vrs`]; the rubric is invoked via
    /// [`RecomputeHook::recompute_confidence`]. The variant exists only so a VR5
    /// deviation can be labeled — it adds no new rule (VR5 is already in
    /// `schema-formalization.md`).
    Vr5,
    /// VR7 — recompute `technical_assessment` and compare. This is the label for
    /// the recompute-and-compare rule whose **deterministic** portion is
    /// implemented by the Task 7 verdict engine (see [`crate::verdict`]). It is
    /// **not** run by [`run_vrs`]; the engine is invoked via
    /// [`RecomputeHook::recompute_verdict`]. The variant exists only so a VR7
    /// deviation can be labeled — it adds no new rule (VR7 is already in
    /// `schema-formalization.md`).
    Vr7,
    /// VR8 — every gate appears in ≥1 coverage entry.
    Vr8,
    /// VR9 — no "correct/safe" adjudication language in rationale/summary.
    Vr9,
    /// VR11 — `EpistemicState.conclusion=VERIFIED` references ≥1 execution-based
    /// facet (OBSERVED, or executed-PASSED TESTED). *(correction 1)*
    Vr11,
    /// VR12 — every `ExecutionResult.command_id` / executed reference resolves to
    /// a VerificationMechanism with `declared_by_project=true`. *(correction 3)*
    Vr12,
    /// VR13 — no report/persistence path targets a location under
    /// `project_root`. *(correction 4)*
    Vr13,
}

impl Vr {
    /// Stable identifier string naming the VR (used in error reporting so a
    /// rejection names the rule it violated — AC-6).
    pub fn id(self) -> &'static str {
        match self {
            Vr::Vr1 => "VR1",
            Vr::Vr2 => "VR2",
            Vr::Vr5 => "VR5",
            Vr::Vr7 => "VR7",
            Vr::Vr8 => "VR8",
            Vr::Vr9 => "VR9",
            Vr::Vr11 => "VR11",
            Vr::Vr12 => "VR12",
            Vr::Vr13 => "VR13",
        }
    }
}

/// A single rule violation, naming the VR, a locator into the report, and a
/// human-readable detail. The `vr` field lets callers/tests assert the precise
/// rejection site (AC-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The rule that was violated.
    pub vr: Vr,
    /// A locator into the report (e.g. `findings[0]`, `gates[1].name`) that
    /// identifies where the violation was found.
    pub locator: String,
    /// A human-readable explanation of the violation.
    pub detail: String,
}

impl Violation {
    pub(crate) fn new(vr: Vr, locator: impl Into<String>, detail: impl Into<String>) -> Self {
        Violation {
            vr,
            locator: locator.into(),
            detail: detail.into(),
        }
    }
}

impl core::fmt::Display for Violation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} at {}: {}", self.vr.id(), self.locator, self.detail)
    }
}

/// Context that a few VRs need without granting CORE any host capability.
///
/// The values are supplied by the caller (a runtime/adapter in later tasks). In
/// Task 2 tests they are supplied directly from the report under test. CORE
/// never reads the filesystem or spawns processes to obtain them — keeping this
/// crate platform-independent (Layer ownership, CAP-1).
pub trait ValidationContext {
    /// The audited project's root path, as a normalized string, for VR13
    /// path-containment. CORE treats this as an opaque string prefix; it does
    /// not resolve it against a real filesystem.
    fn project_root(&self) -> &str;

    /// The set of `VerificationMechanism.id` values known to be
    /// `declared_by_project=true`, for VR12 foreign-key resolution.
    fn known_mechanism_ids(&self) -> &[String];
}

/// A minimal, self-contained [`ValidationContext`] value type.
///
/// This is a convenience for callers (and Task 2 tests) that already hold the
/// two context values; it carries no host capability of its own.
#[derive(Debug, Clone, Default)]
pub struct StaticValidationContext {
    /// Project root used for VR13 path-containment.
    pub project_root: String,
    /// Ids of project-declared verification mechanisms, for VR12.
    pub known_mechanism_ids: Vec<String>,
}

impl StaticValidationContext {
    /// Build a context from a report value by reading `request.project_root` and
    /// the ids of `verification_mechanisms` with `declared_by_project=true`.
    ///
    /// This is pure JSON inspection — no host access. It mirrors what a runtime
    /// binding will later assemble from the real request and discovered
    /// registry, so Task 2 fixtures exercise the same shape.
    pub fn from_report(report: &Value) -> Self {
        let project_root = report
            .get("request")
            .and_then(|r| r.get("project_root"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let known_mechanism_ids = report
            .get("verification_mechanisms")
            .and_then(Value::as_array)
            .map(|mechs| {
                mechs
                    .iter()
                    .filter(|m| {
                        m.get("declared_by_project")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    })
                    .filter_map(|m| m.get("id").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        StaticValidationContext {
            project_root,
            known_mechanism_ids,
        }
    }
}

impl ValidationContext for StaticValidationContext {
    fn project_root(&self) -> &str {
        &self.project_root
    }

    fn known_mechanism_ids(&self) -> &[String] {
        &self.known_mechanism_ids
    }
}

/// Recompute-and-compare hook for VR5 (confidence) and VR7 (verdict).
///
/// **Task 2 provides only this surface.** The confidence rubric (Task 6) and the
/// verdict engine (Task 7) plug in here later. Task 2 ships a no-op
/// implementation ([`NoRecompute`]) and does **not** implement any rubric or
/// verdict computation. `run_vrs` does not invoke recomputation; the hook exists
/// so a later task can attach without changing this module's public shape.
pub trait RecomputeHook {
    /// Recompute confidence for each finding and compare against the recorded
    /// value (VR5). Deferred to Task 6/15. The default implementation performs
    /// no recomputation and returns no violations.
    fn recompute_confidence(&self, _report: &Value) -> Vec<Violation> {
        Vec::new()
    }

    /// Recompute the technical assessment via the verdict engine and compare
    /// (VR7). Deferred to Task 7/15. The default implementation performs no
    /// recomputation and returns no violations.
    fn recompute_verdict(&self, _report: &Value) -> Vec<Violation> {
        Vec::new()
    }
}

/// The Task 2 no-op recompute hook: implements neither the rubric nor the
/// verdict engine. It exists only so the plug-in point is stable and testable.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRecompute;

impl RecomputeHook for NoRecompute {}

/// Run the semantic / cross-reference portion of VR1, VR2, VR8, VR9, VR11,
/// VR12, VR13 over a canonical [`AuditReport`](crate::report) value.
///
/// Returns all violations found (empty ⇒ the report passes every semantic VR
/// this validator owns). Structural rules VR3/VR4/VR6 are assumed already
/// enforced by JSON Schema validation upstream (including VR10's enum guard);
/// VR5/VR7 recomputation is deferred (see [`RecomputeHook`]) and is **not** run
/// here.
pub fn run_vrs(report: &Value, ctx: &dyn ValidationContext) -> Vec<Violation> {
    let mut violations = Vec::new();
    violations.extend(rules::vr01_sufficiency::check(report));
    violations.extend(rules::vr02_failure_anchor::check(report));
    violations.extend(rules::vr08_gate_coverage::check(report));
    violations.extend(rules::vr09_language::check(report));
    violations.extend(rules::vr11_verified_facet::check(report));
    violations.extend(rules::vr12_command_fk::check(report, ctx));
    violations.extend(rules::vr13_write_path::check(report, ctx));
    violations
}
