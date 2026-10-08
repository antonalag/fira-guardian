//! Execution-grounded gate evaluation for the **Tests** and **Build** gates
//! (vertical slice; §6 outcome→epistemic + §7 decision table, narrowed to the
//! two mechanism-backed gates).
//!
//! This is the first step that derives a **real** gate state from evidence
//! rather than leaving every gate `UNKNOWN`. It is a pure, deterministic CORE
//! function over the raw [`ExecutionResult`]s the runtime already produces for
//! project-declared mechanisms; it performs no I/O and reads no narrative
//! (blind-first, C9 — preserved by omission).
//!
//! Scope (deliberately narrow):
//! - Only the `Tests` and `Build` gates may change from `UNKNOWN`; every other
//!   gate stays `UNKNOWN` ("not sufficiently audited", C11). Findings are not
//!   produced here.
//! - The mechanism→gate association lives in CORE (not POLICY): it is the §6
//!   domain mapping "what a declared test/build mechanism's outcome means for
//!   its gate", keyed off the mechanism's structural id. Only explicitly
//!   recognized (canonical) mechanisms back a gate; anything else backs nothing.
//!
//! Fail-safe invariants (never violated here):
//! - **C5:** `NOT_RUN`/`TIMEOUT`/`BLOCKED`/`ERROR` and absent/`INSUFFICIENT`
//!   evidence never yield `PASS` (the gate stays `UNKNOWN`).
//! - **FAILED ⇒ FAIL** for the backed gate (app-level negative evidence).
//! - **VR1:** a `PASS` gate carries ≥1 `SUFFICIENT` support mapping.
//! - **VR11 / C6:** the only `VERIFIED` epistemic conclusion emitted here is
//!   grounded in an execution-based `OBSERVED` facet from a real `PASSED` run —
//!   never static, and scoped to the behavior that execution exercised, never
//!   generalized to whole-implementation correctness.
//! - **VR12:** evidence references a discovered, `declared_by_project=true`
//!   mechanism (the evidence is built from executed registry entries; nothing is
//!   synthesized).

use crate::epistemic::EpistemicState;
use crate::evidence::SupportMapping;
use crate::execution::{ExecutionResult, VerificationMechanism};
use crate::gate::Gate;
use crate::model::{
    EpistemicConclusion, ExecutionOutcome, Facet, GateCause, GateName, GateState, IdRef,
    RequirementLevel, Sufficiency,
};

/// One executed mechanism's raw result paired with its declared registry record.
/// Both already exist; this is just an input bundle (no new domain type).
#[derive(Debug, Clone, Copy)]
pub struct ExecutedMechanismResult<'a> {
    pub mechanism: &'a VerificationMechanism,
    pub result: &'a ExecutionResult,
}

/// The gate (if any) that a discovered mechanism backs, from its **structural
/// id** alone (the §7.1 canonical association). Only the project's canonical
/// test / build mechanism backs a gate; variants (`npm-test:coverage`), linters,
/// typecheckers, watchers, and unknown ids back **nothing** (⇒ `None`), so they
/// can never infer a gate PASS.
///
/// Keyed on the id discovery assigns (`cargo-test`/`npm-test`/`make-test`;
/// `cargo-build`/`npm-build`/`make-build`). This is CORE domain logic, not POLICY
/// data and not a schema field.
pub fn backing_gate(mechanism: &VerificationMechanism) -> Option<GateName> {
    match mechanism.id.0.as_str() {
        "cargo-test" | "npm-test" | "make-test" => Some(GateName::Tests),
        "cargo-build" | "npm-build" | "make-build" => Some(GateName::Build),
        _ => None,
    }
}

/// Evaluate the execution-backed gates (`Tests`, `Build`) from the executed
/// mechanisms. Returns one [`Gate`] per *backed, profile-present* gate, in a
/// deterministic order (`Build` then `Tests`, by [`GateName`]). Gates the caller
/// knows about but that are not returned here stay `UNKNOWN` (the caller fills
/// them in), as do backed gates with no qualifying execution evidence.
///
/// Pure and deterministic (C10): the output is a function of the inputs only; no
/// clock, randomness, filesystem, process, or network.
///
/// `profile_gates` supplies the requirement level + whether the gate exists in
/// the applied profile (a gate absent from the profile, or `not_applicable`, is
/// never produced here — the caller handles N/A as a profile fact).
pub fn evaluate_execution_backed_gates(
    profile_gates: &[(GateName, RequirementLevel)],
    executed: &[ExecutedMechanismResult<'_>],
) -> Vec<Gate> {
    let mut out: Vec<Gate> = Vec::new();

    // Deterministic iteration over the two slice gates only.
    for gate_name in [GateName::Build, GateName::Tests] {
        // Must be present in the applied profile and applicable (not N/A).
        let Some((_, level)) = profile_gates.iter().find(|(g, _)| *g == gate_name) else {
            continue;
        };
        if *level == RequirementLevel::NotApplicable {
            continue;
        }

        // The canonical mechanism(s) backing this gate, in input order (stable).
        let backing: Vec<&ExecutedMechanismResult<'_>> = executed
            .iter()
            .filter(|e| backing_gate(e.mechanism) == Some(gate_name))
            .collect();
        if backing.is_empty() {
            // No recognized backing mechanism ⇒ leave UNKNOWN (caller emits it).
            continue;
        }

        out.push(evaluate_one(gate_name, *level, &backing));
    }

    out
}

/// Evaluate a single backed gate from its backing mechanism results, applying
/// the §6 outcome→epistemic subset, the VR1 sufficiency rule, and the §7
/// decision table (PASS / FAIL / UNKNOWN only — no PARTIAL in this slice).
fn evaluate_one(
    gate_name: GateName,
    requirement_level: RequirementLevel,
    backing: &[&ExecutedMechanismResult<'_>],
) -> Gate {
    // App-level negative evidence dominates (fail-safe): if any backing
    // mechanism FAILED, the gate FAILs regardless of other PASSES (§7.2).
    let any_failed = backing
        .iter()
        .any(|e| e.result.outcome == ExecutionOutcome::Failed);
    let any_passed = backing
        .iter()
        .any(|e| e.result.outcome == ExecutionOutcome::Passed);

    let mut support_mappings: Vec<SupportMapping> = Vec::new();
    for e in backing {
        support_mappings.push(support_mapping_for(e));
    }

    let (state, cause, rationale) = if any_failed {
        (
            GateState::Fail,
            Some(GateCause::App),
            format!(
                "the project's declared {} mechanism executed and reported failure \
                 (app-level negative evidence)",
                gate_label(gate_name)
            ),
        )
    } else if any_passed && has_sufficient(&support_mappings) {
        (
            GateState::Pass,
            Some(GateCause::None),
            format!(
                "the project's declared {} mechanism executed and passed; this is \
                 execution-grounded evidence for that mechanism only and does not \
                 establish behavior-level verification of unexercised properties",
                gate_label(gate_name)
            ),
        )
    } else {
        // Backed, but no qualifying execution evidence (e.g. NOT_RUN / TIMEOUT /
        // BLOCKED / ERROR) ⇒ UNKNOWN, never PASS (C5).
        (
            GateState::Unknown,
            Some(GateCause::None),
            format!(
                "the project's declared {} mechanism did not produce a conclusive \
                 execution result (not sufficiently audited)",
                gate_label(gate_name)
            ),
        )
    };

    let _ = requirement_level; // requirement level is the verdict engine's input, not a gate-state input here.

    Gate {
        name: gate_name,
        requirement_level,
        state,
        rationale,
        cause,
        support_mappings,
        supporting_findings: Vec::new(),
    }
}

/// Build the [`SupportMapping`] for one backing mechanism result. The mapping's
/// `sufficiency` is `SUFFICIENT` only for a definite `PASSED`/`FAILED` outcome
/// (the mechanism ran to a verdict); any non-conclusive outcome is
/// `INSUFFICIENT` with an `unverified_remainder` (so a PASS can never rest on it
/// — VR1 + C5). The single evidence ref is the executed mechanism's `command_id`
/// (a COMMAND evidence reference to a declared mechanism — VR12).
fn support_mapping_for(e: &ExecutedMechanismResult<'_>) -> SupportMapping {
    let conclusive = matches!(
        e.result.outcome,
        ExecutionOutcome::Passed | ExecutionOutcome::Failed
    );
    let sufficiency = if conclusive {
        Sufficiency::Sufficient
    } else {
        Sufficiency::Insufficient
    };
    let unverified_remainder = if conclusive {
        None
    } else {
        Some(format!(
            "mechanism did not run to a conclusive outcome ({:?}); no execution evidence",
            e.result.outcome
        ))
    };
    SupportMapping {
        claim: format!(
            "declared mechanism {} executed with outcome {:?}",
            e.mechanism.id.0, e.result.outcome
        ),
        evidence_refs: vec![e.mechanism.id.clone()],
        relevant_span: e.mechanism.source_locator.clone(),
        sufficiency,
        unverified_remainder,
    }
}

/// The execution-grounded [`EpistemicState`] for a `PASSED` mechanism: an
/// `OBSERVED` (execution-based) facet with a `VERIFIED` conclusion, scoped to the
/// behavior the execution exercised. Exposed for the caller/tests; a `PASSED`
/// gate's VERIFIED conclusion is only ever reached through this (never static —
/// C6/VR11). Non-passed outcomes have no VERIFIED conclusion.
pub fn observed_state_for(result: &ExecutionResult) -> Option<EpistemicState> {
    if result.outcome == ExecutionOutcome::Passed {
        Some(EpistemicState {
            // OBSERVED = the mechanism was executed and observed to pass. Scoped
            // to the exercised behavior only (the facet does not assert whole-
            // implementation correctness).
            facets: vec![Facet::Observed],
            conclusion: EpistemicConclusion::Verified,
        })
    } else {
        None
    }
}

fn has_sufficient(mappings: &[SupportMapping]) -> bool {
    mappings
        .iter()
        .any(|m| m.sufficiency == Sufficiency::Sufficient)
}

fn gate_label(g: GateName) -> &'static str {
    match g {
        GateName::Tests => "test",
        GateName::Build => "build",
        _ => "verification",
    }
}

/// Evidence-ref id helper so callers can cross-check the FK (VR12) without
/// reaching into the mapping internals.
pub fn backing_command_ids(gate: &Gate) -> Vec<IdRef> {
    gate.support_mappings
        .iter()
        .flat_map(|m| m.evidence_refs.iter().cloned())
        .collect()
}
