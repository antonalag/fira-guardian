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
//! Failure classification (failure-classification-slice, tables T-G / T-P):
//! - Each backing result contributes to its gate according to its `outcome` and
//!   `classification` **only** (never captured output, command text or blocking
//!   detail): positive (PASSED, unclassified), negative with APP attribution
//!   (FAILED/ERROR + `APP_LEVEL`), negative without established attribution
//!   (FAILED + `UNCLASSIFIED` or absent), inconclusive with an infrastructure
//!   impediment (FAILED/ERROR/BLOCKED + `INFRA_LEVEL`), or inconclusive.
//! - A backing mechanism that produced **no** execution result is inconclusive.
//! - Precedence: any inconclusive contribution ⇒ `UNKNOWN` (cause `INFRA` iff an
//!   infrastructure impediment is present, else `NONE`); otherwise any negative
//!   contribution ⇒ `FAIL` (cause `APP` iff an APP attribution is present, else
//!   `NONE`); otherwise `PASS` only if every contribution is positive and a
//!   `SUFFICIENT` mapping exists; anything else ⇒ `UNKNOWN`.
//!
//! Fail-safe invariants (never violated here):
//! - **C5:** an inconclusive backing result (NOT_RUN / TIMEOUT / BLOCKED /
//!   unattributed ERROR / a missing result / a contradictory classification)
//!   never yields `PASS`, alone or alongside a PASSED sibling.
//! - **No unsupported attribution:** `cause = APP` only with a positive
//!   APP_LEVEL classification; a FAILED without established attribution is a
//!   `FAIL` with cause `NONE` (no application defect has been established).
//! - **Negative evidence is retained:** its mapping stays `SUFFICIENT` even when
//!   the gate ends `UNKNOWN`.
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
    EpistemicConclusion, ExecutionClassification, ExecutionOutcome, Facet, GateCause, GateName,
    GateState, IdRef, RequirementLevel, Sufficiency,
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
///
/// Equivalent to [`evaluate_execution_backed_gates_with_unexecuted`] with no
/// backing mechanism lacking an execution result.
pub fn evaluate_execution_backed_gates(
    profile_gates: &[(GateName, RequirementLevel)],
    executed: &[ExecutedMechanismResult<'_>],
) -> Vec<Gate> {
    evaluate_execution_backed_gates_with_unexecuted(profile_gates, executed, &[])
}

/// Like [`evaluate_execution_backed_gates`], additionally taking the discovered
/// mechanisms for which **no execution result exists** (the executor returned an
/// error instead of a result). Each such mechanism counts, only for the gate it
/// backs (per [`backing_gate`]), as an inconclusive backing: that gate can never
/// be `PASS` and ends `UNKNOWN` with an explicit rationale. A mechanism without
/// a result that backs no gate, or backs a gate absent from the profile or
/// `not_applicable`, affects nothing.
pub fn evaluate_execution_backed_gates_with_unexecuted(
    profile_gates: &[(GateName, RequirementLevel)],
    executed: &[ExecutedMechanismResult<'_>],
    without_result: &[&VerificationMechanism],
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
        let missing: Vec<&VerificationMechanism> = without_result
            .iter()
            .copied()
            .filter(|m| backing_gate(m) == Some(gate_name))
            .collect();
        if backing.is_empty() && missing.is_empty() {
            // No recognized backing mechanism ⇒ leave UNKNOWN (caller emits it).
            continue;
        }

        out.push(evaluate_one(gate_name, *level, &backing, &missing));
    }

    out
}

/// How one backing result (or a missing result) contributes to its gate
/// (table T-G).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Contribution {
    /// PASSED with no classification.
    Positive,
    /// Negative evidence with a positive APP_LEVEL attribution.
    NegativeApp,
    /// Negative evidence whose attribution is not established.
    NegativeUnattributed,
    /// Inconclusive because of an infrastructure impediment (INFRA_LEVEL).
    InconclusiveInfra,
    /// Inconclusive without attribution (incl. a missing result).
    Inconclusive,
}

/// Table T-G. Reads **only** `outcome` and `classification`. Every combination
/// is enumerated explicitly (no wildcard) so a new outcome or classification
/// value fails to compile until its meaning is decided.
fn contribution(r: &ExecutionResult) -> Contribution {
    use ExecutionClassification as C;
    use ExecutionOutcome as O;
    match (r.outcome, r.classification) {
        (O::Passed, None) => Contribution::Positive,
        // Contradictory input (a pass carrying a classification): never PASS.
        (O::Passed, Some(C::AppLevel) | Some(C::InfraLevel) | Some(C::Unclassified)) => {
            Contribution::Inconclusive
        }
        (O::Failed, Some(C::AppLevel)) => Contribution::NegativeApp,
        (O::Failed, Some(C::InfraLevel)) => Contribution::InconclusiveInfra,
        (O::Failed, Some(C::Unclassified) | None) => Contribution::NegativeUnattributed,
        // §6: ERROR + APP is FAILED-like.
        (O::Error, Some(C::AppLevel)) => Contribution::NegativeApp,
        (O::Error, Some(C::InfraLevel)) => Contribution::InconclusiveInfra,
        (O::Error, Some(C::Unclassified) | None) => Contribution::Inconclusive,
        // §6/§11: BLOCKED by an infrastructure impediment is an operational
        // blocker; otherwise inconclusive.
        (O::Blocked, Some(C::InfraLevel)) => Contribution::InconclusiveInfra,
        (O::Blocked, Some(C::AppLevel) | Some(C::Unclassified) | None) => {
            Contribution::Inconclusive
        }
        // A timeout or a non-execution is neither APP nor INFRA.
        (O::Timeout, Some(C::AppLevel) | Some(C::InfraLevel) | Some(C::Unclassified) | None) => {
            Contribution::Inconclusive
        }
        (O::NotRun, Some(C::AppLevel) | Some(C::InfraLevel) | Some(C::Unclassified) | None) => {
            Contribution::Inconclusive
        }
    }
}

/// Evaluate a single backed gate from its backing results and its backing
/// mechanisms without a result, applying T-G, the VR1 sufficiency rule, and
/// the T-P precedence (PASS / FAIL / UNKNOWN only — no PARTIAL in this slice).
fn evaluate_one(
    gate_name: GateName,
    requirement_level: RequirementLevel,
    backing: &[&ExecutedMechanismResult<'_>],
    missing: &[&VerificationMechanism],
) -> Gate {
    let mut contributions: Vec<Contribution> = Vec::new();
    let mut support_mappings: Vec<SupportMapping> = Vec::new();
    for e in backing {
        let c = contribution(e.result);
        contributions.push(c);
        support_mappings.push(support_mapping_for(e, c));
    }
    for m in missing {
        contributions.push(Contribution::Inconclusive);
        support_mappings.push(support_mapping_without_result(m));
    }

    let any_inconclusive = contributions.iter().any(|c| {
        matches!(
            c,
            Contribution::InconclusiveInfra | Contribution::Inconclusive
        )
    });
    let any_infra = contributions.contains(&Contribution::InconclusiveInfra);
    let any_negative = contributions.iter().any(|c| {
        matches!(
            c,
            Contribution::NegativeApp | Contribution::NegativeUnattributed
        )
    });
    let any_app = contributions.contains(&Contribution::NegativeApp);
    let all_positive =
        !contributions.is_empty() && contributions.iter().all(|c| *c == Contribution::Positive);

    let label = gate_label(gate_name);
    let (state, cause, mut rationale) = if any_inconclusive {
        if any_infra {
            (
                GateState::Unknown,
                GateCause::Infra,
                format!(
                    "the project's declared {label} mechanism did not produce a conclusive \
                     result because of an infrastructure impediment (INFRA_LEVEL) identified \
                     by the execution layer; not sufficiently audited"
                ),
            )
        } else {
            (
                GateState::Unknown,
                GateCause::None,
                unknown_none_rationale(label),
            )
        }
    } else if any_negative {
        if any_app {
            (
                GateState::Fail,
                GateCause::App,
                format!(
                    "the project's declared {label} mechanism executed and reported failure \
                     classified APP_LEVEL (application-level attribution established by the \
                     execution layer)"
                ),
            )
        } else {
            (
                GateState::Fail,
                GateCause::None,
                format!(
                    "the project's declared {label} mechanism executed and the verification \
                     failed; the attribution of the failure (application or infrastructure) \
                     is not established (UNCLASSIFIED); no application defect has been \
                     established"
                ),
            )
        }
    } else if all_positive && has_sufficient(&support_mappings) {
        (
            GateState::Pass,
            GateCause::None,
            format!(
                "the project's declared {label} mechanism executed and passed; this is \
                 execution-grounded evidence for that mechanism only and does not \
                 establish behavior-level verification of unexercised properties"
            ),
        )
    } else {
        // No evaluable backing, or no SUFFICIENT mapping ⇒ never PASS (C5/VR1).
        (
            GateState::Unknown,
            GateCause::None,
            unknown_none_rationale(label),
        )
    };

    if state == GateState::Unknown {
        if any_negative {
            rationale.push_str(
                "; negative execution evidence from at least one declared mechanism is \
                 retained but does not determine this gate because another result is \
                 inconclusive",
            );
        }
        if !missing.is_empty() {
            rationale.push_str(
                "; at least one declared mechanism could not be executed and has no \
                 execution result",
            );
        }
    }

    Gate {
        name: gate_name,
        requirement_level,
        state,
        rationale,
        cause: Some(cause),
        support_mappings,
        supporting_findings: Vec::new(),
    }
}

fn unknown_none_rationale(label: &str) -> String {
    format!(
        "the project's declared {label} mechanism did not produce a conclusive \
         execution result (not sufficiently audited)"
    )
}

/// Build the [`SupportMapping`] for one backing mechanism result. The mapping's
/// `sufficiency` is `SUFFICIENT` only for a positive or negative contribution
/// (the mechanism ran to a verdict whose meaning is established); an
/// inconclusive contribution is `INSUFFICIENT` with an `unverified_remainder`
/// (so a PASS can never rest on it — VR1 + C5). The single evidence ref is the
/// executed mechanism's `command_id` (a COMMAND evidence reference to a
/// declared mechanism — VR12).
fn support_mapping_for(e: &ExecutedMechanismResult<'_>, c: Contribution) -> SupportMapping {
    let (sufficiency, unverified_remainder) = match c {
        Contribution::Positive | Contribution::NegativeApp | Contribution::NegativeUnattributed => {
            (Sufficiency::Sufficient, None)
        }
        Contribution::InconclusiveInfra => (
            Sufficiency::Insufficient,
            Some(format!(
                "mechanism could not be evaluated because of an infrastructure impediment \
                 ({:?}, INFRA_LEVEL); it provides no evidence about the application",
                e.result.outcome
            )),
        ),
        Contribution::Inconclusive => (
            Sufficiency::Insufficient,
            Some(format!(
                "mechanism did not run to a conclusive outcome ({:?}); no execution evidence",
                e.result.outcome
            )),
        ),
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

/// The [`SupportMapping`] for a backing mechanism that has no execution result.
/// Always `INSUFFICIENT`: it provides no evidence.
fn support_mapping_without_result(m: &VerificationMechanism) -> SupportMapping {
    SupportMapping {
        claim: format!(
            "declared mechanism {} could not be executed; no execution result exists",
            m.id.0
        ),
        evidence_refs: vec![m.id.clone()],
        relevant_span: m.source_locator.clone(),
        sufficiency: Sufficiency::Insufficient,
        unverified_remainder: Some(
            "mechanism could not be executed; no execution result exists; it provides no \
             evidence"
                .to_string(),
        ),
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

#[cfg(test)]
mod tests {
    use super::evaluate_one;
    use crate::model::{GateCause, GateName, GateState, RequirementLevel};

    /// The aggregation never yields PASS from an empty contribution set (the
    /// public entry points never call it that way; this pins the defensive
    /// branch, which is otherwise unreachable through the public API).
    #[test]
    fn empty_backing_is_unknown_never_pass() {
        let g = evaluate_one(GateName::Tests, RequirementLevel::Required, &[], &[]);
        assert_eq!(g.state, GateState::Unknown);
        assert_eq!(g.cause, Some(GateCause::None));
        assert!(g.support_mappings.is_empty());
    }
}
