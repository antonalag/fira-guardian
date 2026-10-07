//! P1 system classification (§12): a pure, deterministic map from structural
//! manifest signals to a system class (`ProfileId`), or an explicit
//! `Undetermined` outcome when signals are absent or conflicting.
//!
//! Generic classification only: this module names `ProfileId` (a CORE enum) and
//! never references `AuditProfile` or any POLICY type, so the frozen
//! `POLICY → CORE` dependency direction is preserved. Profile resolution and the
//! confirm-gate projection that consume `AuditProfile` live in POLICY.
//!
//! Classification uses **structural signals only** (§12 P1): manifest
//! presence/role flags, never README / design docs / narrative (the P2 forbidden
//! sources). The input type carries no such field, so a forbidden source cannot
//! influence the result.

use crate::model::ProfileId;

/// Structural signals permitted by §12 P1. Presence/role flags detected from
/// manifests by the caller (a RUNTIME `RepositoryReader`); CORE reads no files.
///
/// There is deliberately no field for README/doc/narrative text: the forbidden
/// P2 sources are unrepresentable here and therefore cannot affect `classify`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClassificationSignals {
    /// A `Cargo.toml` manifest is present.
    pub has_cargo_toml: bool,
    /// A `package.json` manifest is present.
    pub has_package_json: bool,
    /// A `pom.xml` manifest is present.
    pub has_pom_xml: bool,
    /// A `build.gradle` manifest is present.
    pub has_build_gradle: bool,
    /// A `go.mod` manifest is present.
    pub has_go_mod: bool,
    /// A `Dockerfile` is present.
    pub has_dockerfile: bool,
    /// A CI configuration is present.
    pub has_ci_config: bool,
    /// A manifest declares a library / exported-lib target.
    pub declares_library_target: bool,
    /// A manifest declares an executable / bin / CLI entrypoint.
    pub declares_binary_target: bool,
    /// A manifest/CI indicates a long-running service (web framework, exposed
    /// port, service-style start command).
    pub indicates_service: bool,
    /// A manifest/CI indicates a scheduled/triggered batch job.
    pub indicates_batch_job: bool,
    /// Signals of durable state / distribution (datastore client, clustering).
    pub indicates_stateful_distribution: bool,
}

impl ClassificationSignals {
    /// True if at least one recognized structural manifest is present. Used only
    /// to describe "no recognized manifest at all" in an `Undetermined`
    /// rationale; it is never itself a class signal.
    fn any_manifest_present(&self) -> bool {
        self.has_cargo_toml
            || self.has_package_json
            || self.has_pom_xml
            || self.has_build_gradle
            || self.has_go_mod
            || self.has_dockerfile
            || self.has_ci_config
    }
}

/// The outcome of P1 classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// A single class is clearly indicated by the structural signals.
    Classified {
        profile_id: ProfileId,
        /// Why this class was chosen (the matched rule). A reviewable audit
        /// trail, not a model-authored field.
        rationale: String,
    },
    /// Signals are absent or conflict with no decisive rule; the human must
    /// choose at the confirm gate (no silent default).
    Undetermined {
        /// Why classification could not settle on one class.
        rationale: String,
    },
}

/// Classify a project from its structural signals (§12 P1).
///
/// Pure and deterministic: same signals ⇒ same result; no clock, randomness,
/// filesystem, process, or network. First-match order (approved §9.1):
/// `stateful_distributed` → `web_service` → `batch_pipeline` → `cli_tool` →
/// `library`; otherwise `Undetermined`. Manifest language is never itself a
/// class signal; there is no scoring, weighting, or tie-breaking beyond this
/// order.
pub fn classify(signals: &ClassificationSignals) -> Classification {
    // 1. stateful_distributed — strongest/most-consequential signal first.
    if signals.indicates_stateful_distribution {
        return Classification::Classified {
            profile_id: ProfileId::StatefulDistributed,
            rationale: "durable-state / distribution signals present".to_string(),
        };
    }

    // 2. web_service.
    if signals.indicates_service {
        return Classification::Classified {
            profile_id: ProfileId::WebService,
            rationale: "service signals present (framework / port / service start)".to_string(),
        };
    }

    // 3. batch_pipeline.
    if signals.indicates_batch_job {
        return Classification::Classified {
            profile_id: ProfileId::BatchPipeline,
            rationale: "batch-job signals present (schedule / batch runner)".to_string(),
        };
    }

    // 4. cli_tool — a binary target with no library target and no service/batch
    //    character (service/batch already handled above).
    if signals.declares_binary_target && !signals.declares_library_target {
        return Classification::Classified {
            profile_id: ProfileId::CliTool,
            rationale: "binary target with no library/service/batch character".to_string(),
        };
    }

    // 5. library — a library target with no binary target and no
    //    service/batch/stateful character (all handled above).
    if signals.declares_library_target && !signals.declares_binary_target {
        return Classification::Classified {
            profile_id: ProfileId::Library,
            rationale: "library target with no runnable/service character".to_string(),
        };
    }

    // 6. Undetermined — no match. Distinguish "no recognized manifest" from
    //    "conflicting/ambiguous signals" in the rationale (both defer to human).
    let rationale = if !signals.any_manifest_present() {
        "no recognized structural manifest; human must choose a class".to_string()
    } else {
        "structural signals are absent or conflicting; human must choose a class".to_string()
    };
    Classification::Undetermined { rationale }
}
