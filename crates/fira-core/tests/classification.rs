//! Task 9 CORE tests: the P1 classifier over structural signals.
//!
//! - AC-1: pure/deterministic; maps signals to a ProfileId or explicit Undetermined.
//! - AC-2: a case per §9.1 rule-table branch + Undetermined cases.
//! - AC-3: structural-signals-only (the input type carries no forbidden-source
//!   field — enforced at the type level; exercised here with only structural
//!   flags).

use fira_core::classification::{classify, Classification, ClassificationSignals};
use fira_core::model::ProfileId;

fn classified_as(signals: &ClassificationSignals) -> Option<ProfileId> {
    match classify(signals) {
        Classification::Classified { profile_id, .. } => Some(profile_id),
        Classification::Undetermined { .. } => None,
    }
}

/// Rule 1: durable/distribution signal ⇒ stateful_distributed.
#[test]
fn stateful_distributed_wins() {
    let s = ClassificationSignals {
        indicates_stateful_distribution: true,
        ..Default::default()
    };
    assert_eq!(classified_as(&s), Some(ProfileId::StatefulDistributed));
}

/// Precedence: stateful outranks every other signal present simultaneously.
#[test]
fn stateful_distributed_outranks_others() {
    let s = ClassificationSignals {
        indicates_stateful_distribution: true,
        indicates_service: true,
        indicates_batch_job: true,
        declares_binary_target: true,
        declares_library_target: true,
        ..Default::default()
    };
    assert_eq!(classified_as(&s), Some(ProfileId::StatefulDistributed));
}

/// Rule 2: service signal (without stateful) ⇒ web_service, even if it also
/// ships a binary.
#[test]
fn web_service_wins_over_binary() {
    let s = ClassificationSignals {
        indicates_service: true,
        declares_binary_target: true,
        ..Default::default()
    };
    assert_eq!(classified_as(&s), Some(ProfileId::WebService));
}

/// Rule 3: batch signal (without stateful/service) ⇒ batch_pipeline.
#[test]
fn batch_pipeline_wins_over_binary() {
    let s = ClassificationSignals {
        indicates_batch_job: true,
        declares_binary_target: true,
        ..Default::default()
    };
    assert_eq!(classified_as(&s), Some(ProfileId::BatchPipeline));
}

/// Rule 4: binary target, no library target, no service/batch/stateful ⇒ cli_tool.
#[test]
fn cli_tool_for_binary_only() {
    let s = ClassificationSignals {
        has_cargo_toml: true,
        declares_binary_target: true,
        ..Default::default()
    };
    assert_eq!(classified_as(&s), Some(ProfileId::CliTool));
}

/// Rule 5: library target, no binary target, no service/batch/stateful ⇒ library.
#[test]
fn library_for_library_only() {
    let s = ClassificationSignals {
        has_cargo_toml: true,
        declares_library_target: true,
        ..Default::default()
    };
    assert_eq!(classified_as(&s), Some(ProfileId::Library));
}

/// Rule 6: no recognized manifest and no role signal ⇒ Undetermined.
#[test]
fn undetermined_when_no_signals() {
    let s = ClassificationSignals::default();
    match classify(&s) {
        Classification::Undetermined { rationale } => {
            assert!(rationale.contains("no recognized"), "rationale: {rationale}");
        }
        other => panic!("expected Undetermined, got {other:?}"),
    }
}

/// Rule 6: conflicting binary+library (with a manifest present) ⇒ Undetermined,
/// with the "conflicting" rationale (not the "no manifest" one).
#[test]
fn undetermined_when_binary_and_library_conflict() {
    let s = ClassificationSignals {
        has_package_json: true,
        declares_binary_target: true,
        declares_library_target: true,
        ..Default::default()
    };
    match classify(&s) {
        Classification::Undetermined { rationale } => {
            assert!(rationale.contains("conflicting"), "rationale: {rationale}");
        }
        other => panic!("expected Undetermined, got {other:?}"),
    }
}

/// A bare manifest with no role signal is still Undetermined: manifest language
/// is never itself a class signal (no hidden language-based classification).
#[test]
fn manifest_language_alone_is_not_a_class_signal() {
    for s in [
        ClassificationSignals { has_cargo_toml: true, ..Default::default() },
        ClassificationSignals { has_package_json: true, ..Default::default() },
        ClassificationSignals { has_pom_xml: true, ..Default::default() },
        ClassificationSignals { has_go_mod: true, ..Default::default() },
    ] {
        assert_eq!(
            classified_as(&s),
            None,
            "a manifest with no role signal must not classify to a language-based class"
        );
    }
}

/// AC-1 determinism: same input ⇒ identical output.
#[test]
fn classification_is_deterministic() {
    let s = ClassificationSignals {
        indicates_service: true,
        has_dockerfile: true,
        ..Default::default()
    };
    assert_eq!(classify(&s), classify(&s));
}
