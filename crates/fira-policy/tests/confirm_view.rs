//! Task 9 POLICY test (AC-4): the confirm-gate view reproduces each profile's
//! gate matrix, focus_areas, typical_failure_modes, and N/A justifications
//! exactly, for all five built-in profiles.

use fira_core::model::RequirementLevel;
use fira_policy::{confirm_gate_view, profile_for, ALL_IDS};

#[test]
fn view_reproduces_each_profile_exactly() {
    for id in ALL_IDS {
        let profile = profile_for(id);
        let view = confirm_gate_view(&profile);

        assert_eq!(view.profile_id, profile.profile_id);
        assert_eq!(view.version, profile.version);
        assert_eq!(view.focus_areas, profile.focus_areas);
        assert_eq!(view.typical_failure_modes, profile.typical_failure_modes);

        // Gate matrix: same gates, same levels, same order as the profile.
        let expected_matrix: Vec<_> = profile
            .gates
            .iter()
            .map(|g| (g.gate, g.requirement_level))
            .collect();
        assert_eq!(view.gate_matrix, expected_matrix, "{id:?} gate matrix");

        // N/A justifications: exactly the not_applicable gates, with their text.
        let expected_na: Vec<_> = profile
            .gates
            .iter()
            .filter(|g| g.requirement_level == RequirementLevel::NotApplicable)
            .map(|g| {
                (
                    g.gate,
                    g.na_justification
                        .clone()
                        .expect("N/A gate carries a justification"),
                )
            })
            .collect();
        assert_eq!(view.na_justifications, expected_na, "{id:?} N/A justifications");

        // Every N/A justification is non-empty.
        for (gate, j) in &view.na_justifications {
            assert!(!j.trim().is_empty(), "{id:?} gate {gate:?} empty N/A justification");
        }
    }
}

/// The convenience resolver matches the explicit resolve+project path.
#[test]
fn view_for_matches_resolve_then_project() {
    for id in ALL_IDS {
        assert_eq!(
            fira_policy::confirm_gate_view_for(id),
            confirm_gate_view(&profile_for(id))
        );
    }
}
