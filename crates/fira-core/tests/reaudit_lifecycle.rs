//! Re-audit lifecycle reconciliation (§14) — behavioral tests for
//! `fira_core::lifecycle::reconcile`.
//!
//! Exercises the approved two-source model (explicit current-round
//! `lifecycle_status` + carried-over prior state) and the fixed 5-rule
//! precedence (design §2.1). Computed statuses are fed through the **unchanged**
//! verdict engine (`compute_assessment`) to assert end-to-end §14 weight.
//! Findings are built from JSON; a prior `AuditReport` is built by swapping the
//! `findings` array of the golden S11 example and deserializing it.

mod common;

use common::load_json;
use serde_json::{json, Value};

use fira_core::finding::Finding;
use fira_core::gate::Gate;
use fira_core::lifecycle::{reconcile, LifecycleOutcome};
use fira_core::model::{LifecycleStatus, TechnicalAssessmentResult};
use fira_core::report::AuditReport;
use fira_core::verdict::{compute_assessment, ExpectationViolations};

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The shared base finding shape: CRITICAL/HIGH/BLOCKER, OPEN, VERIFIED+OBSERVED
/// (a VERIFIED, execution-based finding whose current round meets the bar),
/// affecting the Tests gate. Callers override only the fields they need.
fn base_finding() -> Value {
    json!({
        "id": "RR-001",
        "maturity": "FINDING",
        "title": "t",
        "severity": "CRITICAL",
        "confidence": "HIGH",
        "confidence_derivation": "d",
        "category": "input-validation",
        "requirement_source": "declared_claim",
        "gates_affected": ["Tests"],
        "epistemic_state": { "facets": ["OBSERVED"], "conclusion": "VERIFIED" },
        "support_mappings": [
            { "claim": "c", "evidence_refs": ["E-1"], "relevant_span": "s", "sufficiency": "SUFFICIENT" }
        ],
        "impact": "i",
        "release_impact": "BLOCKER",
        "recommended_remediation": "r",
        "verification_criteria": "re-run cargo test; cases PASS",
        "lifecycle_status": "OPEN",
        "regression": false
    })
}

/// A typed current-round `Finding` with `overrides` applied to the base shape.
fn finding(overrides: Value) -> Finding {
    serde_json::from_value(finding_json(overrides)).expect("finding deserializes")
}

/// A finding as a JSON `Value` (for embedding in a prior report).
fn finding_json(overrides: Value) -> Value {
    let mut base = base_finding();
    merge(&mut base, overrides);
    base
}

/// A prior `AuditReport` whose `findings` array is replaced by `findings`.
fn prior_report(findings: Vec<Value>) -> AuditReport {
    let mut report = load_json("schemas/examples/valid/s11-audit-report.json");
    report["findings"] = Value::Array(findings);
    serde_json::from_value(report).expect("prior AuditReport deserializes")
}

fn gate(name: &str, level: &str, state: &str) -> Gate {
    serde_json::from_value(json!({
        "name": name,
        "requirement_level": level,
        "state": state,
        "rationale": "r",
        "support_mappings": [],
        "supporting_findings": []
    }))
    .expect("gate deserializes")
}

fn merge(base: &mut Value, overrides: Value) {
    let obj = base.as_object_mut().unwrap();
    for (k, v) in overrides.as_object().unwrap() {
        obj.insert(k.clone(), v.clone());
    }
}

fn none() -> ExpectationViolations {
    ExpectationViolations::default()
}

/// The outcome for `id` in a reconcile result.
fn outcome_for<'a>(outcomes: &'a [LifecycleOutcome], id: &str) -> &'a LifecycleOutcome {
    outcomes
        .iter()
        .find(|o| o.id.0 == id)
        .unwrap_or_else(|| panic!("no outcome for {id}"))
}

/// Reconcile a single current finding against an optional single prior finding.
/// Both `current` and `prior` are overrides applied to the base finding shape
/// (so the prior deserializes with all required S7 fields).
fn reconcile_one(current: Value, prior: Option<Value>) -> LifecycleOutcome {
    let c = finding(current);
    let out = match prior {
        Some(p) => reconcile(
            std::slice::from_ref(&c),
            Some(&prior_report(vec![finding_json(p)])),
        ),
        None => reconcile(std::slice::from_ref(&c), None),
    };
    out.into_iter().next().expect("one outcome")
}

/// Apply a computed status onto a finding and run the (unchanged) verdict engine
/// over a single PASS/required Tests gate, returning the result.
fn verdict_of(
    mut f: Finding,
    status: LifecycleStatus,
    regression: bool,
) -> TechnicalAssessmentResult {
    f.lifecycle_status = status;
    f.regression = regression;
    let gates = vec![gate("Tests", "required", "PASS")];
    compute_assessment(&gates, &[f], &none()).result
}

// ---------------------------------------------------------------------------
// AC-1 / AC-2 — new vs carried-over by stable id
// ---------------------------------------------------------------------------

/// AC-1: a current id absent from the prior report is a NEW finding ⇒ OPEN
/// (no explicit current-round signal), never auto-VERIFIED_FIXED.
#[test]
fn new_finding_defaults_open() {
    let o = reconcile_one(json!({ "id": "RR-100", "lifecycle_status": "OPEN" }), None);
    assert_eq!(o.lifecycle_status, LifecycleStatus::Open);
    assert!(!o.regression);
}

/// AC-2: a current id matching a prior id is reconciled against that prior
/// finding and the id is preserved.
#[test]
fn carried_over_matched_by_id() {
    let o = reconcile_one(
        json!({ "id": "RR-001", "lifecycle_status": "OPEN" }),
        Some(json!({ "id": "RR-001", "lifecycle_status": "OPEN" })),
    );
    assert_eq!(o.id.0, "RR-001");
    assert_eq!(o.lifecycle_status, LifecycleStatus::Open);
}

// ---------------------------------------------------------------------------
// AC-7a — first-time entry edges from the explicit current-round signal
// ---------------------------------------------------------------------------

/// AC-7a: a new/OPEN current finding carrying an explicit current-round
/// FIX_CLAIMED (no prior) is a claim ⇒ VERIFIED_FIXED iff the bar is met.
#[test]
fn first_time_fix_claimed_meets_bar() {
    // base meets the bar; execution requirement derived from current (no prior).
    let o = reconcile_one(json!({ "lifecycle_status": "FIX_CLAIMED" }), None);
    assert_eq!(o.lifecycle_status, LifecycleStatus::VerifiedFixed);
    assert!(!o.regression);
}

/// AC-7a: a new/OPEN current finding carrying an explicit current-round
/// WONT_FIX (no prior) ⇒ WONT_FIX, honored as the first-time entry edge.
#[test]
fn first_time_wont_fix() {
    let o = reconcile_one(json!({ "lifecycle_status": "WONT_FIX" }), None);
    assert_eq!(o.lifecycle_status, LifecycleStatus::WontFix);
    assert!(!o.regression);
    // retains blocking weight via the unchanged verdict engine.
    assert_eq!(
        verdict_of(finding(json!({})), LifecycleStatus::WontFix, false),
        TechnicalAssessmentResult::NotReady
    );
}

// ---------------------------------------------------------------------------
// AC-7b — FIX_CLAIMED/WONT_FIX never inferred
// ---------------------------------------------------------------------------

/// AC-7b: a plain OPEN current finding (no explicit signal), present and with no
/// prior, stays OPEN — neither FIX_CLAIMED nor WONT_FIX is manufactured.
#[test]
fn plain_open_stays_open_never_inferred() {
    let o = reconcile_one(json!({ "lifecycle_status": "OPEN" }), None);
    assert_eq!(o.lifecycle_status, LifecycleStatus::Open);
    // Same with a carried-over OPEN prior.
    let o2 = reconcile_one(
        json!({ "lifecycle_status": "OPEN" }),
        Some(json!({ "lifecycle_status": "OPEN" })),
    );
    assert_eq!(o2.lifecycle_status, LifecycleStatus::Open);
}

// ---------------------------------------------------------------------------
// AC-3 — FIX_CLAIMED stays FIX_CLAIMED when the bar is unmet (weight == OPEN)
// ---------------------------------------------------------------------------

/// AC-3: a claimed-but-unverified fix (bar unmet) stays FIX_CLAIMED and the
/// unchanged verdict engine weighs it exactly like OPEN.
#[test]
fn fix_claimed_unmet_bar_stays_fix_claimed_and_blocks_like_open() {
    let f = finding(json!({
        "lifecycle_status": "FIX_CLAIMED",
        "support_mappings": [
            { "claim": "c", "evidence_refs": ["E-1"], "relevant_span": "s", "sufficiency": "INSUFFICIENT" }
        ]
    }));
    let out = reconcile(std::slice::from_ref(&f), None);
    let o = outcome_for(&out, "RR-001");
    assert_eq!(
        o.lifecycle_status,
        LifecycleStatus::FixClaimed,
        "unmet bar ⇒ claim stands"
    );

    let as_claimed = verdict_of(f.clone(), LifecycleStatus::FixClaimed, false);
    let as_open = verdict_of(f, LifecycleStatus::Open, false);
    assert_eq!(as_claimed, TechnicalAssessmentResult::NotReady);
    assert_eq!(as_claimed, as_open, "FIX_CLAIMED weighs like OPEN");
}

// ---------------------------------------------------------------------------
// AC-4 — VERIFIED_FIXED evidence bar (table-driven)
// ---------------------------------------------------------------------------

/// AC-4 (positive): a carried FIX_CLAIMED whose current round meets the full bar
/// (new SUFFICIENT, VERIFIED+OBSERVED, verification_criteria recorded,
/// execution-based because the prior property required execution) ⇒
/// VERIFIED_FIXED, and the verdict engine excludes it ⇒ READY.
#[test]
fn verified_fixed_when_bar_met_carried() {
    let o = reconcile_one(
        json!({}), // base meets the bar
        Some(json!({
            "lifecycle_status": "FIX_CLAIMED",
            "epistemic_state": { "facets": ["OBSERVED"], "conclusion": "VERIFIED" }
        })),
    );
    assert_eq!(o.lifecycle_status, LifecycleStatus::VerifiedFixed);
    assert!(!o.regression);
    assert_eq!(
        verdict_of(finding(json!({})), LifecycleStatus::VerifiedFixed, false),
        TechnicalAssessmentResult::Ready
    );
}

/// AC-4 (negative matrix): each individually-insufficient current state fails
/// the bar ⇒ a carried FIX_CLAIMED does NOT reach VERIFIED_FIXED (it stays
/// FIX_CLAIMED). Prior requires execution (VERIFIED+OBSERVED) so clause (4) is
/// active.
#[test]
fn verified_fixed_bar_negative_matrix() {
    let prior = json!({
        "lifecycle_status": "FIX_CLAIMED",
        "epistemic_state": { "facets": ["OBSERVED"], "conclusion": "VERIFIED" }
    });

    let cases = [
        // INSUFFICIENT support.
        json!({
            "support_mappings": [
                { "claim": "c", "evidence_refs": ["E-1"], "relevant_span": "s", "sufficiency": "INSUFFICIENT" }
            ]
        }),
        // Current conclusion UNVERIFIED.
        json!({ "epistemic_state": { "facets": ["OBSERVED"], "conclusion": "UNVERIFIED" } }),
        // verification_criteria blank.
        json!({ "verification_criteria": "   " }),
        // VERIFIED but NOT execution-based while the original required execution.
        json!({ "epistemic_state": { "facets": ["TESTED"], "conclusion": "VERIFIED" } }),
    ];

    for (i, c) in cases.iter().enumerate() {
        let o = reconcile_one(c.clone(), Some(prior.clone()));
        assert_ne!(
            o.lifecycle_status,
            LifecycleStatus::VerifiedFixed,
            "case {i} must NOT reach VERIFIED_FIXED"
        );
        assert_eq!(
            o.lifecycle_status,
            LifecycleStatus::FixClaimed,
            "case {i} holds the claim at FIX_CLAIMED"
        );
    }
}

/// AC-4 (execution not required): when the prior property did NOT require
/// execution, a current VERIFIED+SUFFICIENT fix with a non-execution facet still
/// meets the bar.
#[test]
fn verified_fixed_when_execution_not_required() {
    let o = reconcile_one(
        json!({ "epistemic_state": { "facets": ["TESTED"], "conclusion": "VERIFIED" } }),
        Some(json!({
            "lifecycle_status": "FIX_CLAIMED",
            "epistemic_state": { "facets": ["IMPLEMENTED"], "conclusion": "UNVERIFIED" }
        })),
    );
    assert_eq!(o.lifecycle_status, LifecycleStatus::VerifiedFixed);
}

// ---------------------------------------------------------------------------
// AC-7c — a current VERIFIED_FIXED is never self-asserted
// ---------------------------------------------------------------------------

/// AC-7c: `current == VERIFIED_FIXED` reaches VERIFIED_FIXED only when the bar is
/// met; with the bar unmet it is a claim ⇒ FIX_CLAIMED. Checked for prior=None
/// and prior=OPEN.
#[test]
fn current_verified_fixed_is_claim_at_most() {
    // Bar unmet (INSUFFICIENT) + self-asserted VERIFIED_FIXED ⇒ FIX_CLAIMED.
    let unmet = json!({
        "lifecycle_status": "VERIFIED_FIXED",
        "support_mappings": [
            { "claim": "c", "evidence_refs": ["E-1"], "relevant_span": "s", "sufficiency": "INSUFFICIENT" }
        ]
    });
    assert_eq!(
        reconcile_one(unmet.clone(), None).lifecycle_status,
        LifecycleStatus::FixClaimed,
        "no prior: self-asserted VERIFIED_FIXED without the bar ⇒ FIX_CLAIMED"
    );
    assert_eq!(
        reconcile_one(unmet, Some(json!({ "lifecycle_status": "OPEN" }))).lifecycle_status,
        LifecycleStatus::FixClaimed,
        "prior OPEN: self-asserted VERIFIED_FIXED without the bar ⇒ FIX_CLAIMED"
    );

    // Bar met + self-asserted VERIFIED_FIXED ⇒ VERIFIED_FIXED (confirmed, not
    // trusted).
    assert_eq!(
        reconcile_one(json!({ "lifecycle_status": "VERIFIED_FIXED" }), None).lifecycle_status,
        LifecycleStatus::VerifiedFixed
    );
}

// ---------------------------------------------------------------------------
// AC-5 — a prior conclusion is not current evidence
// ---------------------------------------------------------------------------

/// AC-5: a prior VERIFIED_FIXED does not keep a recurring finding fixed — it
/// REOPENS with regression, regardless of the prior "conclusion".
#[test]
fn prior_verified_fixed_is_not_current_evidence() {
    let o = reconcile_one(
        json!({}),
        Some(json!({ "lifecycle_status": "VERIFIED_FIXED" })),
    );
    assert_eq!(o.lifecycle_status, LifecycleStatus::Reopened);
    assert!(o.regression);
}

// ---------------------------------------------------------------------------
// AC-6 — regression precedence
// ---------------------------------------------------------------------------

/// AC-6: prior VERIFIED_FIXED present again ⇒ REOPENED + regression; verdict
/// engine evaluates REOPENED as OPEN (NOT_READY).
#[test]
fn regression_reopens_and_blocks() {
    let o = reconcile_one(
        json!({}),
        Some(json!({ "lifecycle_status": "VERIFIED_FIXED" })),
    );
    assert_eq!(o.lifecycle_status, LifecycleStatus::Reopened);
    assert!(o.regression);
    assert_eq!(
        verdict_of(finding(json!({})), o.lifecycle_status, o.regression),
        TechnicalAssessmentResult::NotReady
    );
}

/// AC-6/AC-7d case 4: regression outranks a current FIX_CLAIMED claim.
#[test]
fn regression_outranks_current_fix_claimed() {
    let o = reconcile_one(
        json!({ "lifecycle_status": "FIX_CLAIMED" }),
        Some(json!({ "lifecycle_status": "VERIFIED_FIXED" })),
    );
    assert_eq!(o.lifecycle_status, LifecycleStatus::Reopened);
    assert!(o.regression, "a recurrence is a fact a claim cannot mask");
}

// ---------------------------------------------------------------------------
// AC-7 / AC-7d — WONT_FIX precedence and the six worked edge-case combos
// ---------------------------------------------------------------------------

/// AC-7: carried-over WONT_FIX is preserved when no higher-precedence signal
/// applies, retains severity, and still blocks.
#[test]
fn carried_wont_fix_preserved() {
    let o = reconcile_one(
        json!({ "lifecycle_status": "OPEN" }),
        Some(json!({ "lifecycle_status": "WONT_FIX" })),
    );
    assert_eq!(o.lifecycle_status, LifecycleStatus::WontFix);
    assert!(!o.regression);
    assert_eq!(
        verdict_of(finding(json!({})), LifecycleStatus::WontFix, false),
        TechnicalAssessmentResult::NotReady
    );
}

/// AC-7d: the six approved prior × current edge-case combinations (design
/// §2.1.2). `bar` is satisfied by the base shape unless overridden.
#[test]
fn edge_case_combinations() {
    // (1) prior FIX_CLAIMED, current WONT_FIX ⇒ WONT_FIX (rule 2).
    assert_eq!(
        reconcile_one(
            json!({ "lifecycle_status": "WONT_FIX" }),
            Some(json!({ "lifecycle_status": "FIX_CLAIMED" })),
        )
        .lifecycle_status,
        LifecycleStatus::WontFix
    );

    // (2) prior WONT_FIX, current FIX_CLAIMED, bar met ⇒ VERIFIED_FIXED (rule 3
    //     reached because current != WONT_FIX; carried WONT_FIX at rule 4 is
    //     never consulted).
    assert_eq!(
        reconcile_one(
            json!({ "lifecycle_status": "FIX_CLAIMED" }),
            Some(json!({ "lifecycle_status": "WONT_FIX" })),
        )
        .lifecycle_status,
        LifecycleStatus::VerifiedFixed
    );
    // (2') same but bar unmet ⇒ FIX_CLAIMED.
    assert_eq!(
        reconcile_one(
            json!({
                "lifecycle_status": "FIX_CLAIMED",
                "support_mappings": [
                    { "claim": "c", "evidence_refs": ["E-1"], "relevant_span": "s", "sufficiency": "INSUFFICIENT" }
                ]
            }),
            Some(json!({ "lifecycle_status": "WONT_FIX" })),
        )
        .lifecycle_status,
        LifecycleStatus::FixClaimed
    );

    // (3) prior FIX_CLAIMED, current WONT_FIX, bar WOULD pass ⇒ WONT_FIX (rule 2
    //     before rule 3; the bar is never evaluated).
    assert_eq!(
        reconcile_one(
            json!({ "lifecycle_status": "WONT_FIX" }), // base evidence would pass
            Some(json!({
                "lifecycle_status": "FIX_CLAIMED",
                "epistemic_state": { "facets": ["OBSERVED"], "conclusion": "VERIFIED" }
            })),
        )
        .lifecycle_status,
        LifecycleStatus::WontFix
    );

    // (4) prior VERIFIED_FIXED recurs, current FIX_CLAIMED ⇒ REOPENED+regression
    //     (rule 1). Covered by regression_outranks_current_fix_claimed; asserted
    //     here too for the table's completeness.
    let c4 = reconcile_one(
        json!({ "lifecycle_status": "FIX_CLAIMED" }),
        Some(json!({ "lifecycle_status": "VERIFIED_FIXED" })),
    );
    assert_eq!(c4.lifecycle_status, LifecycleStatus::Reopened);
    assert!(c4.regression);

    // (5) prior OPEN, current VERIFIED_FIXED, bar met ⇒ VERIFIED_FIXED (rule 3,
    //     claim-at-most confirmed by the bar).
    assert_eq!(
        reconcile_one(
            json!({ "lifecycle_status": "VERIFIED_FIXED" }),
            Some(json!({ "lifecycle_status": "OPEN" })),
        )
        .lifecycle_status,
        LifecycleStatus::VerifiedFixed
    );

    // (6) prior None, current VERIFIED_FIXED, bar met ⇒ VERIFIED_FIXED (rule 3,
    //     "requires execution" from the current finding's own epistemic state).
    assert_eq!(
        reconcile_one(json!({ "lifecycle_status": "VERIFIED_FIXED" }), None).lifecycle_status,
        LifecycleStatus::VerifiedFixed
    );
}

// ---------------------------------------------------------------------------
// §9.4 — absence is never VERIFIED_FIXED
// ---------------------------------------------------------------------------

/// §9.4: a prior id absent from the current round yields no outcome and is never
/// minted VERIFIED_FIXED from absence.
#[test]
fn absence_does_not_mint_verified_fixed() {
    let current = vec![finding(
        json!({ "id": "RR-002", "lifecycle_status": "OPEN" }),
    )];
    let prior = prior_report(vec![
        finding_json(json!({ "id": "RR-001", "lifecycle_status": "OPEN" })),
        finding_json(json!({ "id": "RR-003", "lifecycle_status": "VERIFIED_FIXED" })),
    ]);
    let out = reconcile(&current, Some(&prior));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id.0, "RR-002");
    assert_eq!(out[0].lifecycle_status, LifecycleStatus::Open);
    assert!(out
        .iter()
        .all(|o| o.lifecycle_status != LifecycleStatus::VerifiedFixed));
}

// ---------------------------------------------------------------------------
// AC-8 / §9.6 — determinism and iteration-order independence
// ---------------------------------------------------------------------------

/// AC-8: identical inputs ⇒ identical output; a reordered copy of the same
/// inputs ⇒ identical output (results keyed/ordered by FindingId).
#[test]
fn deterministic_and_order_independent() {
    let a = finding(json!({ "id": "RR-001", "lifecycle_status": "WONT_FIX" }));
    let b = finding(json!({ "id": "RR-002" }));
    let c = finding(json!({ "id": "RR-003", "lifecycle_status": "FIX_CLAIMED" }));
    let prior = prior_report(vec![finding_json(
        json!({ "id": "RR-002", "lifecycle_status": "VERIFIED_FIXED" }),
    )]);

    let forward = reconcile(&[a.clone(), b.clone(), c.clone()], Some(&prior));
    let again = reconcile(&[a.clone(), b.clone(), c.clone()], Some(&prior));
    let reordered = reconcile(&[c, b, a], Some(&prior));

    assert_eq!(forward, again, "same inputs ⇒ same output");
    assert_eq!(forward, reordered, "reordered inputs ⇒ same output");

    let ids: Vec<&str> = forward.iter().map(|o| o.id.0.as_str()).collect();
    assert_eq!(ids, vec!["RR-001", "RR-002", "RR-003"]);
}
