//! Task 9 POLICY test (AC-5): PROF-1 human authority is representable but
//! bounded — the human may confirm, reclassify among the five, and override-N/A,
//! but cannot create a sixth profile or redefine gate semantics.

use fira_core::classification::{classify, Classification, ClassificationSignals};
use fira_core::model::{GateName, ProfileId};
use fira_core::report::{NaOverride, NaOverrideBy};
use fira_policy::{AppliedSelection, ALL_IDS};

/// Confirming a Classified proposal applies exactly the proposed class.
#[test]
fn confirm_applies_proposed_class() {
    let proposal = classify(&ClassificationSignals {
        indicates_service: true,
        ..Default::default()
    });
    let applied = AppliedSelection::confirm(proposal.clone())
        .expect("a Classified proposal can be confirmed");
    assert_eq!(applied.applied, ProfileId::WebService);
    assert_eq!(applied.proposal, proposal);
    assert!(applied.na_overrides.is_empty());
}

/// An Undetermined proposal cannot be "confirmed" (there is no proposed class);
/// the human must reclassify.
#[test]
fn undetermined_cannot_be_confirmed() {
    let proposal = classify(&ClassificationSignals::default());
    assert!(matches!(proposal, Classification::Undetermined { .. }));
    assert!(AppliedSelection::confirm(proposal).is_none());
}

/// The human may reclassify to any of the five — including from Undetermined,
/// and overriding a Classified proposal.
#[test]
fn human_may_reclassify_among_the_five() {
    let undetermined = classify(&ClassificationSignals::default());
    for id in ALL_IDS {
        let applied = AppliedSelection::reclassified(undetermined.clone(), id);
        assert_eq!(applied.applied, id);
    }

    // Override a Classified proposal with a different one of the five.
    let proposed_cli = classify(&ClassificationSignals {
        has_cargo_toml: true,
        declares_binary_target: true,
        ..Default::default()
    });
    assert!(matches!(
        proposed_cli,
        Classification::Classified { profile_id: ProfileId::CliTool, .. }
    ));
    let applied = AppliedSelection::reclassified(proposed_cli, ProfileId::Library);
    assert_eq!(applied.applied, ProfileId::Library);
}

/// N/A overrides are representable as {gate, by:human} and resolve a profile.
#[test]
fn na_overrides_are_representable() {
    let proposal = classify(&ClassificationSignals {
        indicates_stateful_distribution: true,
        ..Default::default()
    });
    let applied = AppliedSelection::confirm(proposal)
        .expect("Classified")
        .with_na_override(NaOverride {
            gate: GateName::Observability,
            by: NaOverrideBy::Human,
        });

    assert_eq!(applied.na_overrides.len(), 1);
    assert_eq!(applied.na_overrides[0].gate, GateName::Observability);
    assert_eq!(applied.na_overrides[0].by, NaOverrideBy::Human);

    // The applied class still resolves to one of the five built-in profiles.
    let resolved = applied.applied_profile();
    assert_eq!(resolved.profile_id, ProfileId::StatefulDistributed);
}

/// Structural PROF-1 guard: `applied` is the closed ProfileId enum, so it can
/// only ever be one of the five — a "sixth profile" is unrepresentable. We prove
/// coverage by matching every variant exhaustively.
#[test]
fn applied_class_is_always_one_of_the_five() {
    for id in ALL_IDS {
        let applied = AppliedSelection::reclassified(
            Classification::Undetermined { rationale: "test".to_string() },
            id,
        );
        // Exhaustive match: if a sixth variant were ever added this fails to
        // compile — the type is the guarantee.
        match applied.applied {
            ProfileId::Library
            | ProfileId::CliTool
            | ProfileId::WebService
            | ProfileId::StatefulDistributed
            | ProfileId::BatchPipeline => {}
        }
    }
}
