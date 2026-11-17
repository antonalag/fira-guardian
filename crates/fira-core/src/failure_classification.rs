//! Pure, platform-neutral classification of how an attempt to run a declared
//! verification mechanism ended (failure-classification-slice, table T-C).
//!
//! The execution layer (RUNTIME) observes the operating system and normalizes
//! what it saw into [`RunFacts`]; this module alone decides what those facts
//! mean as an S5 `(outcome, classification)` pair. It performs no I/O, reads no
//! clock, and never receives captured output, error text, paths or any
//! platform-specific type: every input is a closed `Copy` enum, so narrative or
//! project-controlled text cannot influence the attribution (by construction).
//!
//! Conservative by design:
//! - `INFRA_LEVEL` is produced only for a launcher selected by FIRA that the
//!   operating system reported as not found or not executable. The caller
//!   reports `ProjectPathUnavailable` instead whenever access to the audited
//!   project directory could not be verified, which yields `UNCLASSIFIED`.
//! - A process that ran and ended without success is `UNCLASSIFIED`, never
//!   `APP_LEVEL`. No fact in this module produces `APP_LEVEL`.
//! - A pass, a timeout and a non-execution carry no classification.
//!
//! Every `match` here enumerates its variants explicitly (no wildcard arm), so
//! adding a fact variant fails to compile until its meaning is decided.

use crate::model::{ExecutionClassification, ExecutionOutcome};

/// Normalized, platform-neutral facts about how an attempt to run a declared
/// mechanism ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunFacts {
    /// The process started and ended; `success` is whether its exit status was
    /// successful.
    Completed { success: bool },
    /// The process started and exceeded the execution bound (it was
    /// terminated).
    TimedOut,
    /// The process was never started: the mechanism is structurally
    /// non-terminating.
    NotExecuted,
    /// The process could not be started.
    StartFailed(StartFailure),
}

/// Why a process could not be started, and who selected the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartFailure {
    pub cause: StartFailureCause,
    pub launcher: LauncherProvenance,
}

/// The normalized reason a start attempt failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartFailureCause {
    /// The operating system reported that the program was not found.
    ProgramNotFound,
    /// The operating system reported that the program could not be executed
    /// (permission denied).
    ProgramNotExecutable,
    /// Access to the audited project directory could not be verified (missing
    /// or inaccessible), before or after the start attempt.
    ProjectPathUnavailable,
    /// Any other start failure; carries no positive attribution.
    Other,
}

/// Who chose the program that was launched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherProvenance {
    /// The program literal was written by FIRA (discovery), not by the project.
    SelectedByFira,
    /// Provenance could not be established; treated as untrusted.
    NotEstablished,
}

/// The S5 `(outcome, classification)` pair derived from [`RunFacts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunClassification {
    pub outcome: ExecutionOutcome,
    pub classification: Option<ExecutionClassification>,
}

/// Classify run facts per table T-C. Pure, total and deterministic.
pub fn classify_run(facts: &RunFacts) -> RunClassification {
    match *facts {
        // C1
        RunFacts::Completed { success: true } => RunClassification {
            outcome: ExecutionOutcome::Passed,
            classification: None,
        },
        // C2: a non-successful exit is never attributed to the application.
        RunFacts::Completed { success: false } => RunClassification {
            outcome: ExecutionOutcome::Failed,
            classification: Some(ExecutionClassification::Unclassified),
        },
        // C3: a timeout is neither APP nor INFRA.
        RunFacts::TimedOut => RunClassification {
            outcome: ExecutionOutcome::Timeout,
            classification: None,
        },
        // C4
        RunFacts::NotExecuted => RunClassification {
            outcome: ExecutionOutcome::NotRun,
            classification: None,
        },
        // C5-C10
        RunFacts::StartFailed(StartFailure { cause, launcher }) => {
            let classification = match (cause, launcher) {
                (
                    StartFailureCause::ProgramNotFound | StartFailureCause::ProgramNotExecutable,
                    LauncherProvenance::SelectedByFira,
                ) => ExecutionClassification::InfraLevel,
                (
                    StartFailureCause::ProgramNotFound | StartFailureCause::ProgramNotExecutable,
                    LauncherProvenance::NotEstablished,
                ) => ExecutionClassification::Unclassified,
                (
                    StartFailureCause::ProjectPathUnavailable,
                    LauncherProvenance::SelectedByFira | LauncherProvenance::NotEstablished,
                ) => ExecutionClassification::Unclassified,
                (
                    StartFailureCause::Other,
                    LauncherProvenance::SelectedByFira | LauncherProvenance::NotEstablished,
                ) => ExecutionClassification::Unclassified,
            };
            RunClassification {
                outcome: ExecutionOutcome::Error,
                classification: Some(classification),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_run, LauncherProvenance, RunClassification, RunFacts, StartFailure,
        StartFailureCause,
    };
    use crate::model::{ExecutionClassification as C, ExecutionOutcome as O};

    const CAUSES: [StartFailureCause; 4] = [
        StartFailureCause::ProgramNotFound,
        StartFailureCause::ProgramNotExecutable,
        StartFailureCause::ProjectPathUnavailable,
        StartFailureCause::Other,
    ];
    const LAUNCHERS: [LauncherProvenance; 2] = [
        LauncherProvenance::SelectedByFira,
        LauncherProvenance::NotEstablished,
    ];

    /// Exhaustive, wildcard-free: stops compiling if a cause is added, forcing
    /// `CAUSES` (and the T-C table) to be revisited.
    fn cause_index(c: StartFailureCause) -> usize {
        match c {
            StartFailureCause::ProgramNotFound => 0,
            StartFailureCause::ProgramNotExecutable => 1,
            StartFailureCause::ProjectPathUnavailable => 2,
            StartFailureCause::Other => 3,
        }
    }

    /// Exhaustive, wildcard-free (see `cause_index`).
    fn launcher_index(l: LauncherProvenance) -> usize {
        match l {
            LauncherProvenance::SelectedByFira => 0,
            LauncherProvenance::NotEstablished => 1,
        }
    }

    /// Exhaustive, wildcard-free over `RunFacts`: stops compiling if a fact
    /// variant is added, forcing `all_facts` to be revisited.
    fn fact_kind(f: &RunFacts) -> &'static str {
        match f {
            RunFacts::Completed { success: true } => "completed-success",
            RunFacts::Completed { success: false } => "completed-failure",
            RunFacts::TimedOut => "timed-out",
            RunFacts::NotExecuted => "not-executed",
            RunFacts::StartFailed(StartFailure { .. }) => "start-failed",
        }
    }

    fn start_failed(cause: StartFailureCause, launcher: LauncherProvenance) -> RunFacts {
        RunFacts::StartFailed(StartFailure { cause, launcher })
    }

    fn rc(outcome: O, classification: Option<C>) -> RunClassification {
        RunClassification {
            outcome,
            classification,
        }
    }

    /// Every distinct fact value (2 + 1 + 1 + 4 × 2 = 12).
    fn all_facts() -> Vec<RunFacts> {
        let mut v = vec![
            RunFacts::Completed { success: true },
            RunFacts::Completed { success: false },
            RunFacts::TimedOut,
            RunFacts::NotExecuted,
        ];
        for cause in CAUSES {
            for launcher in LAUNCHERS {
                v.push(start_failed(cause, launcher));
            }
        }
        v
    }

    /// AC-1: table T-C, one literal assertion per row.
    #[test]
    fn classify_run_table_rows_c1_to_c10() {
        use LauncherProvenance::{NotEstablished, SelectedByFira};
        use StartFailureCause::{
            Other, ProgramNotExecutable, ProgramNotFound, ProjectPathUnavailable,
        };

        // C1
        assert_eq!(
            classify_run(&RunFacts::Completed { success: true }),
            rc(O::Passed, None)
        );
        // C2
        assert_eq!(
            classify_run(&RunFacts::Completed { success: false }),
            rc(O::Failed, Some(C::Unclassified))
        );
        // C3
        assert_eq!(classify_run(&RunFacts::TimedOut), rc(O::Timeout, None));
        // C4
        assert_eq!(classify_run(&RunFacts::NotExecuted), rc(O::NotRun, None));
        // C5
        assert_eq!(
            classify_run(&start_failed(ProgramNotFound, SelectedByFira)),
            rc(O::Error, Some(C::InfraLevel))
        );
        // C6
        assert_eq!(
            classify_run(&start_failed(ProgramNotExecutable, SelectedByFira)),
            rc(O::Error, Some(C::InfraLevel))
        );
        // C7
        assert_eq!(
            classify_run(&start_failed(ProgramNotFound, NotEstablished)),
            rc(O::Error, Some(C::Unclassified))
        );
        // C8
        assert_eq!(
            classify_run(&start_failed(ProgramNotExecutable, NotEstablished)),
            rc(O::Error, Some(C::Unclassified))
        );
        // C9 (any provenance)
        assert_eq!(
            classify_run(&start_failed(ProjectPathUnavailable, SelectedByFira)),
            rc(O::Error, Some(C::Unclassified))
        );
        assert_eq!(
            classify_run(&start_failed(ProjectPathUnavailable, NotEstablished)),
            rc(O::Error, Some(C::Unclassified))
        );
        // C10 (any provenance)
        assert_eq!(
            classify_run(&start_failed(Other, SelectedByFira)),
            rc(O::Error, Some(C::Unclassified))
        );
        assert_eq!(
            classify_run(&start_failed(Other, NotEstablished)),
            rc(O::Error, Some(C::Unclassified))
        );
    }

    /// AC-1: every fact variant is enumerated and classified as T-C states.
    #[test]
    fn classify_run_covers_every_fact_variant() {
        // The variant lists are complete: indices are exactly 0..len, and the
        // index functions are exhaustive wildcard-free matches.
        let mut ci: Vec<usize> = CAUSES.iter().map(|c| cause_index(*c)).collect();
        ci.sort_unstable();
        assert_eq!(ci, vec![0, 1, 2, 3], "CAUSES must list every cause once");
        let mut li: Vec<usize> = LAUNCHERS.iter().map(|l| launcher_index(*l)).collect();
        li.sort_unstable();
        assert_eq!(li, vec![0, 1], "LAUNCHERS must list every provenance once");

        let facts = all_facts();
        assert_eq!(facts.len(), 12, "2 + 1 + 1 + 4 x 2 fact values");
        for (i, a) in facts.iter().enumerate() {
            for b in &facts[i + 1..] {
                assert_ne!(a, b, "fact values must be distinct");
            }
        }
        let mut kinds: Vec<&str> = facts.iter().map(fact_kind).collect();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(
            kinds,
            vec![
                "completed-failure",
                "completed-success",
                "not-executed",
                "start-failed",
                "timed-out"
            ],
            "every RunFacts variant is represented"
        );

        // Full expected map (T-C), checked for every fact value.
        for f in &facts {
            let expected = match *f {
                RunFacts::Completed { success: true } => rc(O::Passed, None),
                RunFacts::Completed { success: false } => rc(O::Failed, Some(C::Unclassified)),
                RunFacts::TimedOut => rc(O::Timeout, None),
                RunFacts::NotExecuted => rc(O::NotRun, None),
                RunFacts::StartFailed(StartFailure {
                    cause:
                        StartFailureCause::ProgramNotFound | StartFailureCause::ProgramNotExecutable,
                    launcher: LauncherProvenance::SelectedByFira,
                }) => rc(O::Error, Some(C::InfraLevel)),
                RunFacts::StartFailed(StartFailure {
                    cause:
                        StartFailureCause::ProgramNotFound | StartFailureCause::ProgramNotExecutable,
                    launcher: LauncherProvenance::NotEstablished,
                }) => rc(O::Error, Some(C::Unclassified)),
                RunFacts::StartFailed(StartFailure {
                    cause: StartFailureCause::ProjectPathUnavailable | StartFailureCause::Other,
                    launcher:
                        LauncherProvenance::SelectedByFira | LauncherProvenance::NotEstablished,
                }) => rc(O::Error, Some(C::Unclassified)),
            };
            assert_eq!(classify_run(f), expected, "fact {f:?}");
        }
    }

    /// R1.3 / R1.4 / R1.5: never APP_LEVEL; INFRA_LEVEL only for a FIRA-selected
    /// launcher reported not found / not executable; no classification for
    /// pass, timeout or non-execution.
    #[test]
    fn classify_run_never_returns_app_level() {
        for f in all_facts() {
            let r = classify_run(&f);
            assert_ne!(
                r.classification,
                Some(C::AppLevel),
                "no fact may yield APP_LEVEL: {f:?}"
            );
            let infra_allowed = matches!(
                f,
                RunFacts::StartFailed(StartFailure {
                    cause: StartFailureCause::ProgramNotFound
                        | StartFailureCause::ProgramNotExecutable,
                    launcher: LauncherProvenance::SelectedByFira,
                })
            );
            assert_eq!(
                r.classification == Some(C::InfraLevel),
                infra_allowed,
                "INFRA_LEVEL only with positive, FIRA-provenanced evidence: {f:?}"
            );
            if matches!(r.outcome, O::Passed | O::Timeout | O::NotRun) {
                assert_eq!(r.classification, None, "no classification for {f:?}");
            }
        }
    }

    /// Determinism: the same fact always yields the same classification.
    #[test]
    fn classify_run_is_deterministic() {
        for f in all_facts() {
            let copy = f;
            let first = classify_run(&f);
            for _ in 0..3 {
                assert_eq!(classify_run(&f), first);
                assert_eq!(classify_run(&copy), first);
            }
        }
    }

    /// AC-2 / R1.2 / R1.6: the production part of this file (everything before
    /// the test module) contains no platform/I-O API reference and no wildcard
    /// match arm. Patterns are assembled with `concat!` so the test's own text
    /// never matches them (and the architecture guards never see them).
    #[test]
    fn failure_classification_source_is_platform_neutral() {
        const SOURCE: &str = include_str!("failure_classification.rs");
        let marker = concat!("#[cfg", "(test)]");
        let end = SOURCE
            .find(marker)
            .expect("test module marker present in the source");
        let production = &SOURCE[..end];
        assert!(
            production.contains("pub fn classify_run"),
            "the scanned prefix must contain the production code"
        );

        let forbidden_apis = [
            concat!("std", "::", "fs"),
            concat!("std", "::", "io"),
            concat!("std", "::", "os"),
            concat!("std", "::", "process"),
            concat!("cfg", "(unix)"),
        ];
        for pat in forbidden_apis {
            assert!(
                !production.contains(pat),
                "production code must not reference `{pat}`"
            );
        }

        let wildcard_arms = [
            concat!("_", " =>"),
            concat!("_", "=>"),
            concat!(", ", "_)"),
            concat!("(", "_,"),
        ];
        for pat in wildcard_arms {
            assert!(
                !production.contains(pat),
                "production code must not use a wildcard pattern (`{pat}`)"
            );
        }
    }
}
