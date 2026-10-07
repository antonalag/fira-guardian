//! Task 8 tests for the five built-in audit profiles (S2).
//!
//! - AC-1/AC-4: every profile serializes to valid S2 JSON; N/A gates carry a
//!   non-empty na_justification.
//! - AC-2: lookup is total; the profile set is exactly the five frozen ids.
//! - AC-3: library / stateful_distributed gate matrices match §20 verbatim.
//! - AC-5: no profile is labels-only (distinct gate vectors + distinct,
//!   non-empty focus_areas / typical_failure_modes).
//! - AC-6/AC-9: every gate is one of the eleven, each appearing exactly once;
//!   every level is one of the three.

mod common;

use std::collections::BTreeSet;

use fira_core::model::{GateName, ProfileId, RequirementLevel};
use fira_policy::{all, profile_for, AuditProfile, ALL_IDS, PROFILE_VERSION};

use common::compile_s2_schema;

/// The eleven frozen gates (§20).
const ALL_GATES: [GateName; 11] = [
    GateName::Build,
    GateName::Tests,
    GateName::Requirements,
    GateName::Correctness,
    GateName::FailureRecovery,
    GateName::PersistenceDurability,
    GateName::Concurrency,
    GateName::Security,
    GateName::Observability,
    GateName::Documentation,
    GateName::OperationalReadiness,
];

/// AC-1/AC-4: every profile is valid S2 JSON.
#[test]
fn every_profile_validates_against_s2_schema() {
    let schema = compile_s2_schema();
    for profile in all() {
        let json = serde_json::to_value(&profile).expect("profile serializes");
        let errors: Vec<String> = match schema.validate(&json) {
            Ok(()) => Vec::new(),
            Err(errs) => errs.map(|e| e.to_string()).collect(),
        };
        assert!(
            errors.is_empty(),
            "profile {:?} failed the S2 schema: {errors:?}",
            profile.profile_id
        );
    }
}

/// AC-4: every not_applicable gate carries a non-empty na_justification, and no
/// required/recommended gate carries one.
#[test]
fn na_gates_carry_semantic_justification() {
    for profile in all() {
        for gate in &profile.gates {
            match gate.requirement_level {
                RequirementLevel::NotApplicable => {
                    let j = gate
                        .na_justification
                        .as_deref()
                        .unwrap_or_else(|| panic!(
                            "{:?} gate {:?} is N/A without a justification",
                            profile.profile_id, gate.gate
                        ));
                    assert!(
                        !j.trim().is_empty(),
                        "{:?} gate {:?} has an empty na_justification",
                        profile.profile_id,
                        gate.gate
                    );
                }
                _ => assert!(
                    gate.na_justification.is_none(),
                    "{:?} gate {:?} is not N/A but carries an na_justification",
                    profile.profile_id,
                    gate.gate
                ),
            }
        }
    }
}

/// AC-2: profile_for is total over the closed ProfileId set, and ALL_IDS is
/// exactly the five frozen ids with each profile reporting its own id.
#[test]
fn lookup_is_total_and_covers_exactly_five() {
    let expected: BTreeSet<&str> = ["library", "cli_tool", "web_service", "stateful_distributed", "batch_pipeline"]
        .into_iter()
        .collect();

    assert_eq!(ALL_IDS.len(), 5, "exactly five profile ids");

    let ids: BTreeSet<&str> = ALL_IDS.iter().map(|id| profile_id_wire(*id)).collect();
    assert_eq!(ids, expected, "ALL_IDS is exactly the five frozen ids");

    for id in ALL_IDS {
        let p = profile_for(id);
        assert_eq!(p.profile_id, id, "profile_for({id:?}) returns a matching id");
    }
}

/// AC-3: the library gate matrix equals the §20 reference matrix verbatim.
#[test]
fn library_matrix_matches_contract() {
    use GateName::*;
    use RequirementLevel::*;
    let expected = vec![
        (Build, Required),
        (Tests, Required),
        (Requirements, Required),
        (Correctness, Required),
        (Documentation, Required),
        (Security, Recommended),
        (Concurrency, Recommended),
        (FailureRecovery, Recommended),
        (PersistenceDurability, NotApplicable),
        (Observability, NotApplicable),
        (OperationalReadiness, NotApplicable),
    ];
    assert_matrix(&profile_for(ProfileId::Library), &expected);
}

/// AC-3: the stateful_distributed gate matrix equals §20 verbatim.
#[test]
fn stateful_distributed_matrix_matches_contract() {
    use GateName::*;
    use RequirementLevel::*;
    let expected = vec![
        (Build, Required),
        (Tests, Required),
        (Requirements, Required),
        (Correctness, Required),
        (FailureRecovery, Required),
        (PersistenceDurability, Required),
        (Concurrency, Required),
        (Security, Required),
        (OperationalReadiness, Required),
        (Observability, Recommended),
        (Documentation, Recommended),
    ];
    assert_matrix(&profile_for(ProfileId::StatefulDistributed), &expected);
}

/// AC-6/AC-9: every profile assigns all eleven gates exactly once, using only
/// the frozen gate names and the three requirement levels.
#[test]
fn every_profile_covers_all_gates_once_in_vocab() {
    for profile in all() {
        let mut seen: Vec<GateName> = Vec::new();
        for g in &profile.gates {
            assert!(
                ALL_GATES.contains(&g.gate),
                "{:?} references a gate outside the eleven: {:?}",
                profile.profile_id,
                g.gate
            );
            assert!(
                !seen.contains(&g.gate),
                "{:?} lists gate {:?} more than once",
                profile.profile_id,
                g.gate
            );
            seen.push(g.gate);
            // RequirementLevel is a closed enum of exactly the three legal
            // values; matching it exhaustively documents that intent.
            match g.requirement_level {
                RequirementLevel::Required
                | RequirementLevel::Recommended
                | RequirementLevel::NotApplicable => {}
            }
        }
        assert_eq!(
            profile.gates.len(),
            ALL_GATES.len(),
            "{:?} must assign all eleven gates exactly once",
            profile.profile_id
        );
    }
}

/// AC-5: no profile is labels-only — gate-level vectors are pairwise distinct,
/// and focus_areas / typical_failure_modes are non-empty and pairwise distinct.
#[test]
fn no_labels_only_profiles() {
    let profiles = all();

    for p in &profiles {
        assert!(
            !p.focus_areas.is_empty(),
            "{:?} has empty focus_areas",
            p.profile_id
        );
        assert!(
            !p.typical_failure_modes.is_empty(),
            "{:?} has empty typical_failure_modes",
            p.profile_id
        );
        assert!(
            !p.minimum_evidence_expectations.is_empty(),
            "{:?} has empty minimum_evidence_expectations",
            p.profile_id
        );
    }

    for i in 0..profiles.len() {
        for j in (i + 1)..profiles.len() {
            let a = &profiles[i];
            let b = &profiles[j];
            assert_ne!(
                gate_vector(a),
                gate_vector(b),
                "{:?} and {:?} share an identical gate-level vector",
                a.profile_id,
                b.profile_id
            );
            assert_ne!(
                a.focus_areas, b.focus_areas,
                "{:?} and {:?} share identical focus_areas",
                a.profile_id, b.profile_id
            );
            assert_ne!(
                a.typical_failure_modes, b.typical_failure_modes,
                "{:?} and {:?} share identical typical_failure_modes",
                a.profile_id, b.profile_id
            );
        }
    }
}

/// §9.2: every built-in profile is seeded at 1.0.0.
#[test]
fn every_profile_is_version_1_0_0() {
    assert_eq!(PROFILE_VERSION, "1.0.0");
    for p in all() {
        assert_eq!(p.version, "1.0.0", "{:?} must be version 1.0.0", p.profile_id);
    }
}

/// cli_tool carries the approved correction: Security required, Concurrency
/// recommended, PersistenceDurability + OperationalReadiness N/A.
#[test]
fn cli_tool_reflects_approved_correction() {
    let p = profile_for(ProfileId::CliTool);
    assert_eq!(level_of(&p, GateName::Security), RequirementLevel::Required);
    assert_eq!(level_of(&p, GateName::Concurrency), RequirementLevel::Recommended);
    assert_eq!(
        level_of(&p, GateName::PersistenceDurability),
        RequirementLevel::NotApplicable
    );
    assert_eq!(
        level_of(&p, GateName::OperationalReadiness),
        RequirementLevel::NotApplicable
    );
}

// --- helpers ---------------------------------------------------------------

fn profile_id_wire(id: ProfileId) -> &'static str {
    match id {
        ProfileId::Library => "library",
        ProfileId::CliTool => "cli_tool",
        ProfileId::WebService => "web_service",
        ProfileId::StatefulDistributed => "stateful_distributed",
        ProfileId::BatchPipeline => "batch_pipeline",
    }
}

fn level_of(profile: &AuditProfile, gate: GateName) -> RequirementLevel {
    profile
        .gates
        .iter()
        .find(|g| g.gate == gate)
        .unwrap_or_else(|| panic!("{:?} is missing gate {:?}", profile.profile_id, gate))
        .requirement_level
}

fn gate_vector(profile: &AuditProfile) -> Vec<(GateName, RequirementLevel)> {
    ALL_GATES
        .iter()
        .map(|&g| (g, level_of(profile, g)))
        .collect()
}

fn assert_matrix(profile: &AuditProfile, expected: &[(GateName, RequirementLevel)]) {
    for (gate, level) in expected {
        assert_eq!(
            level_of(profile, *gate),
            *level,
            "{:?} gate {:?} should be {:?}",
            profile.profile_id,
            gate,
            level
        );
    }
    assert_eq!(
        profile.gates.len(),
        expected.len(),
        "{:?} has an unexpected number of gate entries",
        profile.profile_id
    );
}
