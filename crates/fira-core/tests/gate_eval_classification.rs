//! failure-classification-slice Task 2: `gate_eval` consumes the execution
//! classification (table T-G), aggregates per gate by precedence (table T-P),
//! fixes G5 (a PASSED sibling never hides an inconclusive backing result), and
//! treats a backing mechanism without an execution result as inconclusive.
//!
//! Pure CORE tests over programmatically constructed results: no process, FS
//! write or network. Expected values are literals taken from the approved
//! specification (`requirements.md §5`, `design.md §3`).

mod common;

use common::load_json;
use serde_json::{json, Value};

use fira_core::confidence::ConfidenceRubric;
use fira_core::execution::{ExecutionResult, VerificationMechanism};
use fira_core::finding::Finding;
use fira_core::gate::Gate;
use fira_core::gate_eval::{
    evaluate_execution_backed_gates, evaluate_execution_backed_gates_with_unexecuted,
    ExecutedMechanismResult,
};
use fira_core::model::{
    AlwaysTrue, ExecutionClassification as C, ExecutionOutcome as O, GateCause, GateName,
    GateState, HumanDecisionFiraState, IdRef, RequirementLevel, Sufficiency,
    TechnicalAssessmentResult, VerificationMechanismSource,
};
use fira_core::validation::{run_vrs, RecomputeHook, StaticValidationContext};
use fira_core::verdict::{compute_assessment, ExpectationViolations, VerdictEngine};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The canonical ids that back the Tests gate (`backing_gate`). Items are
/// assigned to them cyclically; ids may repeat (gate_eval does not require
/// uniqueness).
const TEST_IDS: [&str; 3] = ["npm-test", "cargo-test", "make-test"];

/// One backing input: an executed result, or a mechanism without a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Run(O, Option<C>),
    NoResult,
}

/// T-G contribution classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Cls {
    P,
    A,
    U,
    I,
    N,
}

const ALL_CLS: [Cls; 5] = [Cls::P, Cls::A, Cls::U, Cls::I, Cls::N];

fn mech(id: &str) -> VerificationMechanism {
    VerificationMechanism {
        id: IdRef(id.to_string()),
        source: VerificationMechanismSource::Npm,
        source_locator: "package.json".to_string(),
        command: format!("npm run {id}"),
        declared_by_project: AlwaysTrue,
    }
}

fn result(id: &str, outcome: O, classification: Option<C>) -> ExecutionResult {
    ExecutionResult {
        command_id: IdRef(id.to_string()),
        command: format!("npm run {id}"),
        outcome,
        classification,
        blocking_condition: None,
        captured_output: String::new(),
        exercised_behaviors: Vec::new(),
    }
}

/// Evaluate a single gate whose backings are `items`, with explicit ids and
/// gate/level. Returns every produced gate.
fn eval_with(profile: &[(GateName, RequirementLevel)], items: &[(&str, Item)]) -> Vec<Gate> {
    let mut runs: Vec<(VerificationMechanism, ExecutionResult)> = Vec::new();
    let mut missing: Vec<VerificationMechanism> = Vec::new();
    for (id, item) in items {
        match item {
            Item::Run(o, c) => runs.push((mech(id), result(id, *o, *c))),
            Item::NoResult => missing.push(mech(id)),
        }
    }
    let executed: Vec<ExecutedMechanismResult<'_>> = runs
        .iter()
        .map(|(m, r)| ExecutedMechanismResult {
            mechanism: m,
            result: r,
        })
        .collect();
    let missing_refs: Vec<&VerificationMechanism> = missing.iter().collect();
    evaluate_execution_backed_gates_with_unexecuted(profile, &executed, &missing_refs)
}

/// Evaluate the Tests gate (at `level`) from `items` (ids assigned cyclically).
fn tests_gate_at(level: RequirementLevel, items: &[Item]) -> Gate {
    let labelled: Vec<(&str, Item)> = items
        .iter()
        .enumerate()
        .map(|(i, it)| (TEST_IDS[i % TEST_IDS.len()], *it))
        .collect();
    let mut gates = eval_with(&[(GateName::Tests, level)], &labelled);
    assert_eq!(gates.len(), 1, "exactly the Tests gate for {items:?}");
    gates.remove(0)
}

fn tests_gate(items: &[Item]) -> Gate {
    tests_gate_at(RequirementLevel::Required, items)
}

fn state_cause(g: &Gate) -> (GateState, GateCause) {
    (g.state, g.cause.expect("gate_eval always sets a cause"))
}

// Short item constructors.
fn p() -> Item {
    Item::Run(O::Passed, None)
}
fn run(o: O, c: Option<C>) -> Item {
    Item::Run(o, c)
}

/// Every classification value, including absence.
const CLASSES: [Option<C>; 4] = [
    None,
    Some(C::AppLevel),
    Some(C::InfraLevel),
    Some(C::Unclassified),
];

/// Table T-G (24 cells), as literals.
const TG: [(O, Option<C>, Cls); 24] = [
    (O::Passed, None, Cls::P),
    (O::Passed, Some(C::AppLevel), Cls::N),
    (O::Passed, Some(C::InfraLevel), Cls::N),
    (O::Passed, Some(C::Unclassified), Cls::N),
    (O::Failed, None, Cls::U),
    (O::Failed, Some(C::AppLevel), Cls::A),
    (O::Failed, Some(C::InfraLevel), Cls::I),
    (O::Failed, Some(C::Unclassified), Cls::U),
    (O::Error, None, Cls::N),
    (O::Error, Some(C::AppLevel), Cls::A),
    (O::Error, Some(C::InfraLevel), Cls::I),
    (O::Error, Some(C::Unclassified), Cls::N),
    (O::Timeout, None, Cls::N),
    (O::Timeout, Some(C::AppLevel), Cls::N),
    (O::Timeout, Some(C::InfraLevel), Cls::N),
    (O::Timeout, Some(C::Unclassified), Cls::N),
    (O::NotRun, None, Cls::N),
    (O::NotRun, Some(C::AppLevel), Cls::N),
    (O::NotRun, Some(C::InfraLevel), Cls::N),
    (O::NotRun, Some(C::Unclassified), Cls::N),
    (O::Blocked, None, Cls::N),
    (O::Blocked, Some(C::AppLevel), Cls::N),
    (O::Blocked, Some(C::InfraLevel), Cls::I),
    (O::Blocked, Some(C::Unclassified), Cls::N),
];

/// The class of an item per the literal T-G table (a missing result is N).
fn class_of(item: Item) -> Cls {
    match item {
        Item::NoResult => Cls::N,
        Item::Run(o, c) => {
            TG.iter()
                .find(|(to, tc, _)| *to == o && *tc == c)
                .expect("every (outcome, classification) pair is in T-G")
                .2
        }
    }
}

/// Representatives of each class (requirements AC-11).
fn reps(k: Cls) -> Vec<Item> {
    match k {
        Cls::P => vec![p()],
        Cls::A => vec![
            run(O::Failed, Some(C::AppLevel)),
            run(O::Error, Some(C::AppLevel)),
        ],
        Cls::U => vec![run(O::Failed, Some(C::Unclassified)), run(O::Failed, None)],
        Cls::I => vec![
            run(O::Failed, Some(C::InfraLevel)),
            run(O::Error, Some(C::InfraLevel)),
            run(O::Blocked, Some(C::InfraLevel)),
        ],
        Cls::N => vec![
            run(O::Timeout, None),
            run(O::NotRun, None),
            run(O::Blocked, None),
            run(O::Error, Some(C::Unclassified)),
            run(O::Error, None),
            run(O::Passed, Some(C::AppLevel)),
            run(O::Passed, Some(C::InfraLevel)),
            run(O::Passed, Some(C::Unclassified)),
            Item::NoResult,
        ],
    }
}

/// T-P precedence rules (requirements §5), as an oracle over a class set.
fn tp_oracle(classes: &[Cls]) -> (GateState, GateCause) {
    let has = |k: Cls| classes.contains(&k);
    if has(Cls::I) || has(Cls::N) {
        let cause = if has(Cls::I) {
            GateCause::Infra
        } else {
            GateCause::None
        };
        (GateState::Unknown, cause)
    } else if has(Cls::A) || has(Cls::U) {
        let cause = if has(Cls::A) {
            GateCause::App
        } else {
            GateCause::None
        };
        (GateState::Fail, cause)
    } else {
        (GateState::Pass, GateCause::None)
    }
}

/// The 31 non-empty subsets of {P, A, U, I, N}.
fn subsets() -> Vec<Vec<Cls>> {
    (1u32..32)
        .map(|mask| {
            ALL_CLS
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, k)| *k)
                .collect()
        })
        .collect()
}

/// Cartesian product of the representatives of `classes` (one per class).
fn combinations(classes: &[Cls]) -> Vec<Vec<Item>> {
    let mut acc: Vec<Vec<Item>> = vec![Vec::new()];
    for k in classes {
        let mut next = Vec::new();
        for prefix in &acc {
            for rep in reps(*k) {
                let mut v = prefix.clone();
                v.push(rep);
                next.push(v);
            }
        }
        acc = next;
    }
    acc
}

/// Every (class set, representative combination) pair.
fn all_combinations() -> Vec<(Vec<Cls>, Vec<Item>)> {
    let mut out = Vec::new();
    for s in subsets() {
        for c in combinations(&s) {
            out.push((s.clone(), c));
        }
    }
    out
}

/// Order/duplication variants of an item list.
fn arrangements(items: &[Item]) -> Vec<Vec<Item>> {
    let original = items.to_vec();
    let mut reversed = items.to_vec();
    reversed.reverse();
    let mut rotated = items.to_vec();
    rotated.rotate_left(1);
    let mut duplicated = items.to_vec();
    duplicated.extend_from_slice(items);
    vec![original, reversed, rotated, duplicated]
}

fn contains_word(haystack: &str, word: &str) -> bool {
    haystack
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| w.eq_ignore_ascii_case(word))
}

// ---------------------------------------------------------------------------
// T-G (AC-10)
// ---------------------------------------------------------------------------

/// AC-10: each of the 24 T-G cells, alone, yields the literal gate state,
/// cause, mapping sufficiency and remainder; plus the missing-result case.
#[test]
fn single_result_table_t_g() {
    assert_eq!(TG.len(), 24);
    // Every (outcome, classification) pair appears exactly once.
    for o in [
        O::Passed,
        O::Failed,
        O::Error,
        O::Timeout,
        O::NotRun,
        O::Blocked,
    ] {
        for c in CLASSES {
            let n = TG.iter().filter(|(to, tc, _)| *to == o && *tc == c).count();
            assert_eq!(n, 1, "({o:?}, {c:?}) must appear once in T-G");
        }
    }

    let expected_for = |k: Cls| -> (GateState, GateCause, Sufficiency) {
        match k {
            Cls::P => (GateState::Pass, GateCause::None, Sufficiency::Sufficient),
            Cls::A => (GateState::Fail, GateCause::App, Sufficiency::Sufficient),
            Cls::U => (GateState::Fail, GateCause::None, Sufficiency::Sufficient),
            Cls::I => (
                GateState::Unknown,
                GateCause::Infra,
                Sufficiency::Insufficient,
            ),
            Cls::N => (
                GateState::Unknown,
                GateCause::None,
                Sufficiency::Insufficient,
            ),
        }
    };

    let cells: Vec<(Item, Cls)> = TG
        .iter()
        .map(|(o, c, k)| (Item::Run(*o, *c), *k))
        .chain(std::iter::once((Item::NoResult, Cls::N)))
        .collect();
    for (item, k) in cells {
        let g = tests_gate(&[item]);
        let (state, cause, sufficiency) = expected_for(k);
        assert_eq!(state_cause(&g), (state, cause), "{item:?} ({k:?})");
        assert_eq!(g.support_mappings.len(), 1, "{item:?}");
        let m = &g.support_mappings[0];
        assert_eq!(m.sufficiency, sufficiency, "{item:?} mapping sufficiency");
        assert_eq!(
            m.unverified_remainder.is_some(),
            sufficiency == Sufficiency::Insufficient,
            "{item:?}: unverified_remainder present iff INSUFFICIENT (S4)"
        );
        assert_eq!(m.evidence_refs, vec![IdRef("npm-test".to_string())]);
    }
}

// ---------------------------------------------------------------------------
// T-P (AC-11)
// ---------------------------------------------------------------------------

/// AC-11: the 31 subsets of {P,A,U,I,N}. The 7 subsets without I or N are
/// checked against literals; the 24 with I or N against the trivial oracle
/// (UNKNOWN; cause INFRA iff I). Every combination of representatives is
/// evaluated in original, reversed, rotated and duplicated order.
#[test]
fn precedence_table_t_p() {
    let all = subsets();
    assert_eq!(all.len(), 31);
    assert_eq!(
        all.iter()
            .filter(|s| s.contains(&Cls::I) || s.contains(&Cls::N))
            .count(),
        24
    );

    let literal: [(&[Cls], GateState, GateCause); 7] = [
        (&[Cls::P], GateState::Pass, GateCause::None),
        (&[Cls::A], GateState::Fail, GateCause::App),
        (&[Cls::U], GateState::Fail, GateCause::None),
        (&[Cls::P, Cls::A], GateState::Fail, GateCause::App),
        (&[Cls::P, Cls::U], GateState::Fail, GateCause::None),
        (&[Cls::A, Cls::U], GateState::Fail, GateCause::App),
        (&[Cls::P, Cls::A, Cls::U], GateState::Fail, GateCause::App),
    ];
    for (s, state, cause) in literal {
        assert_eq!(
            tp_oracle(s),
            (state, cause),
            "oracle agrees with literal {s:?}"
        );
    }

    let mut evaluated = 0usize;
    for (classes, items) in all_combinations() {
        // The representatives really are of the claimed classes.
        let mut seen: Vec<Cls> = items.iter().map(|i| class_of(*i)).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen, classes, "representatives of {classes:?}");

        let expected = tp_oracle(&classes);
        for arrangement in arrangements(&items) {
            let g = tests_gate(&arrangement);
            assert_eq!(
                state_cause(&g),
                expected,
                "classes {classes:?}, items {arrangement:?}"
            );
            evaluated += 1;
        }
    }
    // (1+1)(1+2)(1+2)(1+3)(1+9) - 1 = 719 combinations × 4 arrangements.
    assert_eq!(evaluated, 719 * 4);
}

// ---------------------------------------------------------------------------
// Named cases (requirements §5)
// ---------------------------------------------------------------------------

/// The 27 named cases of the specification, as literals. Rows that list
/// alternatives ("—" or UNCLASSIFIED, …) check every alternative.
#[test]
fn named_cases_table() {
    use GateCause as K;
    use GateState as S;
    let f_un = || vec![run(O::Failed, None), run(O::Failed, Some(C::Unclassified))];
    let f_app = || run(O::Failed, Some(C::AppLevel));
    let e_app = || run(O::Error, Some(C::AppLevel));
    let t = || run(O::Timeout, None);

    let mut rows: Vec<(&str, Vec<Vec<Item>>, GateState, GateCause)> = vec![
        ("PASSED", vec![vec![p()]], S::Pass, K::None),
        (
            "FAILED(— | UNCLASSIFIED)",
            f_un().into_iter().map(|f| vec![f]).collect(),
            S::Fail,
            K::None,
        ),
        ("FAILED(APP)", vec![vec![f_app()]], S::Fail, K::App),
        (
            "FAILED(INFRA)",
            vec![vec![run(O::Failed, Some(C::InfraLevel))]],
            S::Unknown,
            K::Infra,
        ),
        ("ERROR(APP)", vec![vec![e_app()]], S::Fail, K::App),
        (
            "ERROR(INFRA)",
            vec![vec![run(O::Error, Some(C::InfraLevel))]],
            S::Unknown,
            K::Infra,
        ),
        (
            "ERROR(— | UNCLASSIFIED)",
            vec![
                vec![run(O::Error, None)],
                vec![run(O::Error, Some(C::Unclassified))],
            ],
            S::Unknown,
            K::None,
        ),
        (
            "TIMEOUT / NOT_RUN",
            vec![vec![t()], vec![run(O::NotRun, None)]],
            S::Unknown,
            K::None,
        ),
        (
            "BLOCKED(INFRA)",
            vec![vec![run(O::Blocked, Some(C::InfraLevel))]],
            S::Unknown,
            K::Infra,
        ),
        (
            "BLOCKED(— / UNCLASSIFIED / APP)",
            vec![
                vec![run(O::Blocked, None)],
                vec![run(O::Blocked, Some(C::Unclassified))],
                vec![run(O::Blocked, Some(C::AppLevel))],
            ],
            S::Unknown,
            K::None,
        ),
        (
            "without result",
            vec![vec![Item::NoResult]],
            S::Unknown,
            K::None,
        ),
        (
            "PASSED + FAILED(— | UNCLASSIFIED)",
            f_un().into_iter().map(|f| vec![p(), f]).collect(),
            S::Fail,
            K::None,
        ),
        (
            "PASSED + FAILED(APP)",
            vec![vec![p(), f_app()]],
            S::Fail,
            K::App,
        ),
        (
            "PASSED + FAILED(INFRA)",
            vec![vec![p(), run(O::Failed, Some(C::InfraLevel))]],
            S::Unknown,
            K::Infra,
        ),
        (
            "PASSED + ERROR(APP)",
            vec![vec![p(), e_app()]],
            S::Fail,
            K::App,
        ),
        (
            "PASSED + ERROR(INFRA)",
            vec![vec![p(), run(O::Error, Some(C::InfraLevel))]],
            S::Unknown,
            K::Infra,
        ),
        (
            "PASSED + ERROR(UNCLASSIFIED | —)",
            vec![
                vec![p(), run(O::Error, Some(C::Unclassified))],
                vec![p(), run(O::Error, None)],
            ],
            S::Unknown,
            K::None,
        ),
        (
            "PASSED + TIMEOUT / NOT_RUN / BLOCKED(—)",
            vec![
                vec![p(), t()],
                vec![p(), run(O::NotRun, None)],
                vec![p(), run(O::Blocked, None)],
            ],
            S::Unknown,
            K::None,
        ),
        (
            "PASSED + BLOCKED(INFRA)",
            vec![vec![p(), run(O::Blocked, Some(C::InfraLevel))]],
            S::Unknown,
            K::Infra,
        ),
        (
            "PASSED + PASSED with a non-empty classification",
            vec![
                vec![p(), run(O::Passed, Some(C::AppLevel))],
                vec![p(), run(O::Passed, Some(C::InfraLevel))],
                vec![p(), run(O::Passed, Some(C::Unclassified))],
            ],
            S::Unknown,
            K::None,
        ),
        (
            "PASSED + without result",
            vec![vec![p(), Item::NoResult]],
            S::Unknown,
            K::None,
        ),
        (
            "FAILED(— | UNCLASSIFIED) + TIMEOUT",
            f_un().into_iter().map(|f| vec![f, t()]).collect(),
            S::Unknown,
            K::None,
        ),
        (
            "FAILED(APP) + TIMEOUT",
            vec![vec![f_app(), t()]],
            S::Unknown,
            K::None,
        ),
        (
            "ERROR(APP) + TIMEOUT",
            vec![vec![e_app(), t()]],
            S::Unknown,
            K::None,
        ),
        (
            "FAILED(APP) + ERROR(INFRA)",
            vec![vec![f_app(), run(O::Error, Some(C::InfraLevel))]],
            S::Unknown,
            K::Infra,
        ),
        (
            "FAILED(APP) + FAILED(UNCLASSIFIED)",
            vec![vec![f_app(), run(O::Failed, Some(C::Unclassified))]],
            S::Fail,
            K::App,
        ),
        (
            "ERROR(APP) + FAILED(UNCLASSIFIED)",
            vec![vec![e_app(), run(O::Failed, Some(C::Unclassified))]],
            S::Fail,
            K::App,
        ),
    ];
    assert_eq!(rows.len(), 27, "27 named cases");

    for (name, variants, state, cause) in rows.drain(..) {
        for items in variants {
            for arrangement in arrangements(&items) {
                let g = tests_gate(&arrangement);
                assert_eq!(
                    state_cause(&g),
                    (state, cause),
                    "named case `{name}`: {arrangement:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// G5, ERROR variants, mixed outcomes (AC-12, AC-13)
// ---------------------------------------------------------------------------

/// AC-12 (G5): a PASSED sibling never turns an inconclusive backing result into
/// PASS. Cause is INFRA only for an infrastructure impediment.
#[test]
fn passed_plus_inconclusive_is_never_pass() {
    let cases: [(Item, GateCause); 11] = [
        (run(O::Timeout, None), GateCause::None),
        (run(O::NotRun, None), GateCause::None),
        (run(O::Blocked, None), GateCause::None),
        (run(O::Blocked, Some(C::InfraLevel)), GateCause::Infra),
        (run(O::Error, Some(C::Unclassified)), GateCause::None),
        (run(O::Error, None), GateCause::None),
        (run(O::Error, Some(C::InfraLevel)), GateCause::Infra),
        (run(O::Passed, Some(C::AppLevel)), GateCause::None),
        (run(O::Passed, Some(C::InfraLevel)), GateCause::None),
        (run(O::Passed, Some(C::Unclassified)), GateCause::None),
        (Item::NoResult, GateCause::None),
    ];
    for (other, cause) in cases {
        for items in [vec![p(), other], vec![other, p()]] {
            let g = tests_gate(&items);
            assert_ne!(g.state, GateState::Pass, "{items:?} must never PASS");
            assert_eq!(state_cause(&g), (GateState::Unknown, cause), "{items:?}");
        }
    }
}

/// AC-12 (B-3): ERROR variants are not grouped. ERROR(APP) is FAILED-like
/// (FAIL/APP, alone and with a PASSED sibling, SUFFICIENT mapping); ERROR(INFRA)
/// is UNKNOWN/INFRA; ERROR(UNCLASSIFIED | —) is UNKNOWN/NONE.
#[test]
fn error_variants_are_not_grouped() {
    let e_app = run(O::Error, Some(C::AppLevel));
    for items in [vec![e_app], vec![p(), e_app], vec![e_app, p()]] {
        let g = tests_gate(&items);
        assert_eq!(
            state_cause(&g),
            (GateState::Fail, GateCause::App),
            "{items:?}"
        );
        let idx = items.iter().position(|i| *i == e_app).unwrap();
        assert_eq!(
            g.support_mappings[idx].sufficiency,
            Sufficiency::Sufficient,
            "ERROR(APP) keeps its negative evidence"
        );
    }
    let g = tests_gate(&[run(O::Error, Some(C::InfraLevel))]);
    assert_eq!(state_cause(&g), (GateState::Unknown, GateCause::Infra));
    for c in [Some(C::Unclassified), None] {
        let g = tests_gate(&[run(O::Error, c)]);
        assert_eq!(
            state_cause(&g),
            (GateState::Unknown, GateCause::None),
            "{c:?}"
        );
    }
}

/// PASSED + FAILED (no inconclusive result) is FAIL.
#[test]
fn failed_plus_passed_is_fail() {
    let g = tests_gate(&[p(), run(O::Failed, None)]);
    assert_eq!(state_cause(&g), (GateState::Fail, GateCause::None));
    let g = tests_gate(&[run(O::Failed, Some(C::AppLevel)), p()]);
    assert_eq!(state_cause(&g), (GateState::Fail, GateCause::App));
}

/// FAILED + TIMEOUT is UNKNOWN (D-2).
#[test]
fn failed_plus_timeout_is_unknown() {
    for f in [run(O::Failed, None), run(O::Failed, Some(C::AppLevel))] {
        let g = tests_gate(&[f, run(O::Timeout, None)]);
        assert_eq!(
            state_cause(&g),
            (GateState::Unknown, GateCause::None),
            "{f:?}"
        );
    }
}

/// AC-13: when a negative result shares the gate with an inconclusive one, the
/// gate is UNKNOWN but the negative evidence is retained (SUFFICIENT mapping)
/// and the rationale says so.
#[test]
fn unknown_gate_retains_negative_mapping_and_says_so() {
    for (negative, outcome_text) in [
        (run(O::Failed, Some(C::AppLevel)), "outcome Failed"),
        (run(O::Failed, Some(C::Unclassified)), "outcome Failed"),
        (run(O::Error, Some(C::AppLevel)), "outcome Error"),
    ] {
        let g = tests_gate(&[negative, run(O::Timeout, None)]);
        assert_eq!(g.state, GateState::Unknown, "{negative:?}");
        let m = &g.support_mappings[0];
        assert_eq!(m.sufficiency, Sufficiency::Sufficient, "{negative:?}");
        assert!(m.claim.contains(outcome_text), "{}", m.claim);
        assert!(m.unverified_remainder.is_none());
        assert!(
            g.rationale.contains("retained"),
            "rationale must state the retained negative evidence: {}",
            g.rationale
        );
    }
}

// ---------------------------------------------------------------------------
// Independence from non-classification fields (AC-3, CORE part)
// ---------------------------------------------------------------------------

/// AC-3: varying `captured_output`, `blocking_condition` and `command` never
/// changes the evaluated gate, for any of the 24 T-G cells.
#[test]
fn gate_ignores_non_classification_fields() {
    let outputs = [
        "",
        "command not found",
        "No such file or directory",
        "permission denied",
        "EACCES",
        "npm ERR! missing script",
        "AssertionError: expected 1 got 2",
        "thread 'main' panicked at src/lib.rs:1",
        "ECONNREFUSED 127.0.0.1:5432",
        "app error",
    ];
    let blocking = [
        None,
        Some("failed to start `make`: No such file or directory (os error 2)"),
        Some("exceeded 300s execution safety bound; process terminated"),
        Some("infrastructure"),
    ];
    let commands = ["npm run npm-test", "make test", "cargo test --all"];
    let m = mech("npm-test");
    let profile = [(GateName::Tests, RequirementLevel::Required)];

    for (o, c, _) in TG {
        let base = result("npm-test", o, c);
        let expected = evaluate_execution_backed_gates(
            &profile,
            &[ExecutedMechanismResult {
                mechanism: &m,
                result: &base,
            }],
        );
        for out in outputs {
            for b in blocking {
                for cmd in commands {
                    let mut r = base.clone();
                    r.captured_output = out.to_string();
                    r.blocking_condition = b.map(str::to_string);
                    r.command = cmd.to_string();
                    let got = evaluate_execution_backed_gates(
                        &profile,
                        &[ExecutedMechanismResult {
                            mechanism: &m,
                            result: &r,
                        }],
                    );
                    assert_eq!(
                        got, expected,
                        "({o:?}, {c:?}) with {out:?} / {b:?} / {cmd:?}"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cause rules (AC-28)
// ---------------------------------------------------------------------------

/// AC-28: cause APP ⇒ an APP attribution (A) is present and the gate is FAIL;
/// and conversely a FAIL with an A has cause APP.
#[test]
fn cause_app_only_with_positive_app_attribution() {
    for (classes, items) in all_combinations() {
        let g = tests_gate(&items);
        let has_a = classes.contains(&Cls::A);
        if g.cause == Some(GateCause::App) {
            assert!(has_a, "APP cause without APP attribution: {items:?}");
            assert_eq!(
                g.state,
                GateState::Fail,
                "APP cause only on FAIL: {items:?}"
            );
        }
        if g.state == GateState::Fail && has_a {
            assert_eq!(g.cause, Some(GateCause::App), "{items:?}");
        }
        if !has_a {
            assert_ne!(g.cause, Some(GateCause::App), "{items:?}");
        }
    }
}

/// AC-28: cause INFRA ⇔ an infrastructure impediment (I) is present; and then
/// the gate is UNKNOWN.
#[test]
fn cause_infra_only_with_infra_contribution() {
    for (classes, items) in all_combinations() {
        let g = tests_gate(&items);
        let has_i = classes.contains(&Cls::I);
        assert_eq!(g.cause == Some(GateCause::Infra), has_i, "{items:?}");
        if has_i {
            assert_eq!(g.state, GateState::Unknown, "{items:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// Backing mechanisms without an execution result (AC-26, CORE part)
// ---------------------------------------------------------------------------

/// AC-26: a backing mechanism without an execution result prevents PASS and is
/// stated explicitly; with retained negative evidence the gate is UNKNOWN.
#[test]
fn without_result_backing_prevents_pass() {
    let g = tests_gate(&[p(), Item::NoResult]);
    assert_eq!(state_cause(&g), (GateState::Unknown, GateCause::None));
    let missing = &g.support_mappings[1];
    assert_eq!(missing.sufficiency, Sufficiency::Insufficient);
    assert!(missing.unverified_remainder.is_some());
    assert!(
        missing.claim.contains("could not be executed"),
        "{}",
        missing.claim
    );
    assert_eq!(missing.evidence_refs, vec![IdRef("cargo-test".to_string())]);
    assert!(
        g.rationale.contains("could not be executed"),
        "{}",
        g.rationale
    );

    let g = tests_gate(&[Item::NoResult]);
    assert_eq!(state_cause(&g), (GateState::Unknown, GateCause::None));

    let g = tests_gate(&[run(O::Failed, Some(C::AppLevel)), Item::NoResult]);
    assert_eq!(g.state, GateState::Unknown);
    assert!(g.rationale.contains("retained"), "{}", g.rationale);
    assert!(
        g.rationale.contains("could not be executed"),
        "{}",
        g.rationale
    );
}

/// A mechanism without a result affects only the gate it backs: a missing
/// build mechanism leaves the Tests gate untouched; a non-backing mechanism
/// affects nothing; a gate absent from the profile or N/A is never produced.
#[test]
fn without_result_only_affects_the_gate_it_backs() {
    let both = [
        (GateName::Build, RequirementLevel::Required),
        (GateName::Tests, RequirementLevel::Required),
    ];

    let gates = eval_with(&both, &[("npm-test", p()), ("npm-build", Item::NoResult)]);
    let tests = gates.iter().find(|g| g.name == GateName::Tests).unwrap();
    let build = gates.iter().find(|g| g.name == GateName::Build).unwrap();
    assert_eq!(state_cause(tests), (GateState::Pass, GateCause::None));
    assert_eq!(state_cause(build), (GateState::Unknown, GateCause::None));
    assert_eq!(build.support_mappings.len(), 1);
    assert_eq!(tests.support_mappings.len(), 1);

    let gates = eval_with(&both, &[("npm-test", p()), ("npm-lint", Item::NoResult)]);
    assert_eq!(
        gates.len(),
        1,
        "a non-backing missing mechanism produces no gate"
    );
    assert_eq!(state_cause(&gates[0]), (GateState::Pass, GateCause::None));

    let tests_only = [(GateName::Tests, RequirementLevel::Required)];
    let gates = eval_with(&tests_only, &[("npm-build", Item::NoResult)]);
    assert!(gates.is_empty(), "Build is absent from the profile");

    let build_na = [
        (GateName::Build, RequirementLevel::NotApplicable),
        (GateName::Tests, RequirementLevel::Required),
    ];
    let gates = eval_with(
        &build_na,
        &[("make-build", Item::NoResult), ("make-test", p())],
    );
    assert_eq!(gates.len(), 1);
    assert_eq!(gates[0].name, GateName::Tests);
    assert_eq!(state_cause(&gates[0]), (GateState::Pass, GateCause::None));
}

// ---------------------------------------------------------------------------
// Rationale texts (design §3.3) and VR9
// ---------------------------------------------------------------------------

const PASS_TEXT: &str = "the project's declared test mechanism executed and passed; this is \
     execution-grounded evidence for that mechanism only and does not establish behavior-level \
     verification of unexercised properties";
const FAIL_APP_TEXT: &str = "the project's declared test mechanism executed and reported failure \
     classified APP_LEVEL (application-level attribution established by the execution layer)";
const FAIL_NONE_TEXT: &str = "the project's declared test mechanism executed and the verification \
     failed; the attribution of the failure (application or infrastructure) is not established \
     (UNCLASSIFIED); no application defect has been established";
const UNKNOWN_INFRA_TEXT: &str = "the project's declared test mechanism did not produce a \
     conclusive result because of an infrastructure impediment (INFRA_LEVEL) identified by the \
     execution layer; not sufficiently audited";
const UNKNOWN_NONE_TEXT: &str = "the project's declared test mechanism did not produce a \
     conclusive execution result (not sufficiently audited)";
const RETAINED_SUFFIX: &str = "; negative execution evidence from at least one declared mechanism \
     is retained but does not determine this gate because another result is inconclusive";
const MISSING_SUFFIX: &str =
    "; at least one declared mechanism could not be executed and has no execution result";

/// Design §3.3: the frozen rationale phrases, exactly; and across every
/// combination: no VR9 adjudication word, and no APP wording unless cause APP.
#[test]
fn rationales_follow_frozen_phrases_and_avoid_vr9_words() {
    let exact: [(Vec<Item>, String); 8] = [
        (vec![p()], PASS_TEXT.to_string()),
        (
            vec![run(O::Failed, Some(C::AppLevel))],
            FAIL_APP_TEXT.to_string(),
        ),
        (vec![run(O::Failed, None)], FAIL_NONE_TEXT.to_string()),
        (
            vec![run(O::Error, Some(C::InfraLevel))],
            UNKNOWN_INFRA_TEXT.to_string(),
        ),
        (vec![run(O::Timeout, None)], UNKNOWN_NONE_TEXT.to_string()),
        (
            vec![run(O::Failed, None), run(O::Timeout, None)],
            format!("{UNKNOWN_NONE_TEXT}{RETAINED_SUFFIX}"),
        ),
        (
            vec![p(), Item::NoResult],
            format!("{UNKNOWN_NONE_TEXT}{MISSING_SUFFIX}"),
        ),
        (
            vec![
                run(O::Failed, Some(C::AppLevel)),
                run(O::Error, Some(C::InfraLevel)),
                Item::NoResult,
            ],
            format!("{UNKNOWN_INFRA_TEXT}{RETAINED_SUFFIX}{MISSING_SUFFIX}"),
        ),
    ];
    for (items, text) in exact {
        assert_eq!(tests_gate(&items).rationale, text, "{items:?}");
    }

    // The label follows the gate.
    let gates = eval_with(
        &[(GateName::Build, RequirementLevel::Required)],
        &[("make-build", run(O::Failed, None))],
    );
    assert_eq!(
        gates[0].rationale,
        FAIL_NONE_TEXT.replace(" test ", " build ")
    );

    for (_, items) in all_combinations() {
        let g = tests_gate(&items);
        for word in ["correct", "safe"] {
            assert!(
                !contains_word(&g.rationale, word),
                "VR9 word `{word}` in {:?}",
                g.rationale
            );
        }
        if g.cause != Some(GateCause::App) {
            assert!(!g.rationale.contains("app-level"), "{}", g.rationale);
            assert!(!g.rationale.contains("APP_LEVEL"), "{}", g.rationale);
        }
        if g.state == GateState::Fail && g.cause == Some(GateCause::None) {
            assert!(g.rationale.contains("attribution of the failure"));
            assert!(g.rationale.contains("is not established"));
            assert!(g
                .rationale
                .contains("no application defect has been established"));
        }
    }
}

// ---------------------------------------------------------------------------
// Verdict composition (AC-14)
// ---------------------------------------------------------------------------

fn assess(gates: &[Gate]) -> fira_core::verdict::RecomputedAssessment {
    compute_assessment(gates, &[], &ExpectationViolations::none())
}

/// AC-14: required FAIL/NONE blocks (B3); required UNKNOWN/INFRA blocks (B4);
/// recommended UNKNOWN is a risk; PASSED + TIMEOUT never reaches READY.
#[test]
fn verdict_composition_required_and_recommended() {
    let g = tests_gate(&[run(O::Failed, None)]);
    let a = assess(&[g]);
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady);
    assert!(a.blocking_gates.contains(&GateName::Tests));

    let g = tests_gate(&[run(O::Error, Some(C::InfraLevel))]);
    let a = assess(&[g]);
    assert_eq!(a.result, TechnicalAssessmentResult::NotReady);
    assert!(a.blocking_gates.contains(&GateName::Tests));

    let g = tests_gate(&[p(), run(O::Timeout, None)]);
    assert_ne!(assess(&[g]).result, TechnicalAssessmentResult::Ready);

    let g = tests_gate_at(RequirementLevel::Recommended, &[run(O::Timeout, None)]);
    let a = assess(&[g]);
    assert_eq!(a.result, TechnicalAssessmentResult::ReadyWithRisks);
    assert!(a.risk_gates.contains(&GateName::Tests));

    // Control: a lone PASSED required gate is READY.
    let g = tests_gate(&[p()]);
    assert_eq!(assess(&[g]).result, TechnicalAssessmentResult::Ready);
}

/// AC-14: the verdict engine does not read `cause`.
#[test]
fn verdict_composition_ignores_cause() {
    for items in [
        vec![run(O::Failed, None)],
        vec![run(O::Failed, Some(C::AppLevel))],
        vec![run(O::Error, Some(C::InfraLevel))],
        vec![p(), run(O::Timeout, None)],
    ] {
        for level in [RequirementLevel::Required, RequirementLevel::Recommended] {
            let g = tests_gate_at(level, &items);
            let baseline = assess(std::slice::from_ref(&g));
            for cause in [
                Some(GateCause::App),
                Some(GateCause::Infra),
                Some(GateCause::None),
                None,
            ] {
                let mut h = g.clone();
                h.cause = cause;
                assert_eq!(assess(&[h]), baseline, "{items:?} / {level:?} / {cause:?}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// VR gauntlet for the new gate shapes (AC-17, CORE part)
// ---------------------------------------------------------------------------

/// Splice `gate` into the golden report, register its backing mechanisms, and
/// record the technical assessment the verdict engine computes for it.
fn report_with_gate(gate: &Gate) -> Value {
    let mut report = load_json("schemas/examples/valid/s11-audit-report.json");
    report["gates"] = Value::Array(vec![serde_json::to_value(gate).expect("gate serializes")]);
    if let Some(mechs) = report["verification_mechanisms"].as_array_mut() {
        for id in TEST_IDS {
            mechs.push(json!({
                "id": id,
                "source": "NPM",
                "source_locator": "package.json",
                "command": format!("npm run {id}"),
                "declared_by_project": true
            }));
        }
    }
    let gates: Vec<Gate> = serde_json::from_value(report["gates"].clone()).unwrap();
    let findings: Vec<Finding> = serde_json::from_value(report["findings"].clone()).unwrap();
    let a = compute_assessment(&gates, &findings, &ExpectationViolations::none());
    report["technical_assessment"]["result"] = serde_json::to_value(a.result).unwrap();
    report["technical_assessment"]["blocking_findings"] =
        serde_json::to_value(&a.blocking_findings).unwrap();
    report["technical_assessment"]["blocking_gates"] =
        serde_json::to_value(&a.blocking_gates).unwrap();
    report["technical_assessment"]["risk_findings"] =
        serde_json::to_value(&a.risk_findings).unwrap();
    report["technical_assessment"]["risk_gates"] = serde_json::to_value(&a.risk_gates).unwrap();
    report["human_decision"]["state"] =
        serde_json::to_value(a.human_decision.unwrap_or(HumanDecisionFiraState::Pending)).unwrap();
    report
}

fn all_violations(report: &Value) -> Vec<fira_core::validation::Violation> {
    let ctx = StaticValidationContext::from_report(report);
    let mut v = run_vrs(report, &ctx);
    v.extend(ConfidenceRubric.recompute_confidence(report));
    v.extend(VerdictEngine.recompute_verdict(report));
    v
}

/// AC-17: FAIL/NONE, FAIL/APP, UNKNOWN/INFRA, UNKNOWN with retained negative
/// evidence and UNKNOWN from a missing result all pass run_vrs (VR1, VR2, VR8,
/// VR9, VR11, VR12, VR13), VR5 and VR7, and the S11 schema.
#[test]
fn evaluated_shapes_pass_run_vrs_and_verdict_recompute() {
    let golden = load_json("schemas/examples/valid/s11-audit-report.json");
    assert!(
        all_violations(&golden).is_empty(),
        "golden report precondition"
    );

    let schema = common::compile_schema_file("s11-audit-report.schema.json");
    let shapes: [(Vec<Item>, GateState, GateCause); 5] = [
        (vec![run(O::Failed, None)], GateState::Fail, GateCause::None),
        (
            vec![run(O::Failed, Some(C::AppLevel))],
            GateState::Fail,
            GateCause::App,
        ),
        (
            vec![run(O::Error, Some(C::InfraLevel))],
            GateState::Unknown,
            GateCause::Infra,
        ),
        (
            vec![run(O::Failed, Some(C::AppLevel)), run(O::Timeout, None)],
            GateState::Unknown,
            GateCause::None,
        ),
        (
            vec![p(), Item::NoResult],
            GateState::Unknown,
            GateCause::None,
        ),
    ];
    for (items, state, cause) in shapes {
        let g = tests_gate(&items);
        assert_eq!(state_cause(&g), (state, cause), "{items:?}");
        let report = report_with_gate(&g);
        let errors: Vec<String> = match schema.validate(&report) {
            Ok(()) => Vec::new(),
            Err(errs) => errs.map(|e| e.to_string()).collect(),
        };
        assert!(errors.is_empty(), "{items:?}: S11 errors {errors:?}");
        let violations = all_violations(&report);
        assert!(violations.is_empty(), "{items:?}: {violations:?}");
    }
}

// ---------------------------------------------------------------------------
// Existing input shapes (classification absent) — no regression
// ---------------------------------------------------------------------------

/// Results with `classification: None` (every result RUNTIME produces today)
/// keep the gate states the previous evaluator produced, and the legacy entry
/// point equals the new one with no missing mechanism.
#[test]
fn existing_input_shapes_unchanged() {
    let both = [
        (GateName::Build, RequirementLevel::Required),
        (GateName::Tests, RequirementLevel::Required),
    ];
    let gates = eval_with(&both, &[("npm-test", p()), ("npm-build", p())]);
    assert!(gates.iter().all(|g| g.state == GateState::Pass));

    let g = tests_gate(&[run(O::Failed, None)]);
    assert_eq!(g.state, GateState::Fail);

    for o in [O::NotRun, O::Timeout, O::Blocked, O::Error] {
        let g = tests_gate(&[run(o, None)]);
        assert_eq!(g.state, GateState::Unknown, "{o:?}");
        assert!(g
            .support_mappings
            .iter()
            .all(|m| m.sufficiency == Sufficiency::Insufficient));
    }

    let g = tests_gate(&[p(), run(O::Failed, None)]);
    assert_eq!(g.state, GateState::Fail);

    // Legacy entry point ≡ new entry point with `without_result = []`.
    let m = mech("npm-test");
    let profile = [(GateName::Tests, RequirementLevel::Required)];
    for (o, c, _) in TG {
        let r = result("npm-test", o, c);
        let executed = [ExecutedMechanismResult {
            mechanism: &m,
            result: &r,
        }];
        assert_eq!(
            evaluate_execution_backed_gates(&profile, &executed),
            evaluate_execution_backed_gates_with_unexecuted(&profile, &executed, &[]),
            "({o:?}, {c:?})"
        );
    }
}
