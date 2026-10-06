//! Validation-rule (VR) enforcement — the semantic / cross-reference portion.
//!
//! VR1–VR13 enforcement is split between the JSON Schema files (structural rules
//! VR3, VR4, VR6, VR10) and this module (VR1, VR2, VR8, VR9, VR11, VR12, VR13).
//! VR5 and VR7 are recompute-and-compare rules invoked via [`RecomputeHook`],
//! not by [`run_vrs`]. Every function here is a pure computation over a parsed
//! [`serde_json::Value`] plus a [`ValidationContext`]; no FS/process/network.
//!
//! VR10 has no variant here on purpose: its escape clause ("a code-internal
//! correctness invariant violation") maps onto the `inferred_invariant`
//! requirement_source, and the Finding model carries no field that distinguishes
//! a qualifying violation from any other `inferred_invariant`-sourced finding.
//! The distinction is therefore not representable, and VR10 is left to the
//! schema's enum guards rather than a semantic check that would need a new field.

use serde_json::Value;

mod rules;

/// The validation rules this validator can label. Structural rules VR3/VR4/VR6
/// are schema-only and absent here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vr {
    /// VR1 — every finding has nonempty `support_mappings`; any PASS/VERIFIED
    /// carries ≥1 SUFFICIENT mapping. (`minItems` is structural; the SUFFICIENT
    /// requirement is semantic.)
    Vr1,
    /// VR2 — concurrency/durability/recovery finding ⇒ `failure_scenario.anchor`
    /// non-null.
    Vr2,
    /// VR5 — recompute confidence and compare. Implemented by the confidence
    /// rubric ([`crate::confidence`]); invoked via
    /// [`RecomputeHook::recompute_confidence`], not by [`run_vrs`]. The variant
    /// only labels a VR5 deviation.
    Vr5,
    /// VR7 — recompute `technical_assessment` and compare. Implemented by the
    /// verdict engine ([`crate::verdict`]); invoked via
    /// [`RecomputeHook::recompute_verdict`], not by [`run_vrs`]. The variant only
    /// labels a VR7 deviation.
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

/// Context that a few VRs need without granting CORE any host capability. The
/// caller supplies the values; CORE never reads the FS or spawns processes to
/// obtain them.
pub trait ValidationContext {
    /// The audited project's root path, as a normalized string, for VR13
    /// path-containment. CORE treats this as an opaque string prefix; it does
    /// not resolve it against a real filesystem.
    fn project_root(&self) -> &str;

    /// The set of `VerificationMechanism.id` values known to be
    /// `declared_by_project=true`, for VR12 foreign-key resolution.
    fn known_mechanism_ids(&self) -> &[String];
}

/// A minimal, self-contained [`ValidationContext`] value type for callers that
/// already hold the two context values. Carries no host capability of its own.
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
    /// Pure JSON inspection, no host access.
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
/// [`run_vrs`] never invokes recomputation; VR5/VR7 are run explicitly through
/// this hook. The default methods are no-ops so a caller can implement only the
/// rule it cares about. The real implementors are [`crate::confidence`] and
/// [`crate::verdict`].
pub trait RecomputeHook {
    /// Recompute confidence for each finding and compare against the recorded
    /// value (VR5). Default: no recomputation, no violations.
    fn recompute_confidence(&self, _report: &Value) -> Vec<Violation> {
        Vec::new()
    }

    /// Recompute the technical assessment and compare (VR7). Default: no
    /// recomputation, no violations.
    fn recompute_verdict(&self, _report: &Value) -> Vec<Violation> {
        Vec::new()
    }
}

/// A no-op recompute hook: implements neither the rubric nor the verdict engine.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRecompute;

impl RecomputeHook for NoRecompute {}

/// Run the semantic / cross-reference portion of VR1, VR2, VR8, VR9, VR11,
/// VR12, VR13 over a canonical [`AuditReport`](crate::report) value.
///
/// Returns all violations found (empty ⇒ the report passes every semantic VR
/// this validator owns). Structural rules VR3/VR4/VR6/VR10 are assumed already
/// enforced by JSON Schema validation upstream; VR5/VR7 are not run here (see
/// [`RecomputeHook`]).
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
