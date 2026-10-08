//! Rendering of the canonical `AuditReport` (S11) to JSON and Markdown.
//!
//! The machine report (JSON) is the single source of truth; Markdown is a
//! read-only human projection. Neither function mutates the report, recomputes a
//! verdict, or injects adjudication ("correct/safe") language — rendering never
//! adjudicates readiness.

use fira_core::report::AuditReport;

/// Render the canonical machine report as pretty JSON. Round-trips: parsing the
/// output yields an equal `AuditReport`.
pub fn render_json(report: &AuditReport) -> String {
    // Serialization cannot fail for this fully-owned, serde-derived type.
    serde_json::to_string_pretty(report).expect("AuditReport serializes to JSON")
}

/// Parse canonical report JSON back into an [`AuditReport`] — the inverse of
/// [`render_json`] (the round-trip the renderer guarantees). Used to load a
/// prior report as historical input (C12); it never adjudicates or mutates.
/// Returns the serde error message on malformed input.
pub fn parse_json(text: &str) -> Result<AuditReport, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

/// Render a read-only human view as Markdown. Displays recorded fields only.
pub fn render_markdown(report: &AuditReport) -> String {
    let mut md = String::new();

    md.push_str(&format!(
        "# FIRA Guardian Audit Report — {}\n\n",
        report.audit_id
    ));
    md.push_str(&format!("- schema version: {}\n", report.schema_version));
    md.push_str(&format!("- created at: {}\n", report.created_at));
    md.push_str(&format!(
        "- release target: {}\n",
        report.request.release_target
    ));
    md.push_str(&format!(
        "- applied profile: {} (v{})\n\n",
        profile_id_label(&report.applied_profile.profile_id),
        report.applied_profile.version
    ));

    md.push_str("## System model summary\n\n");
    md.push_str(&report.system_model_summary);
    md.push_str("\n\n");

    md.push_str("## Technical assessment\n\n");
    md.push_str(&format!(
        "- result: {}\n",
        assessment_label(&report.technical_assessment.result)
    ));
    md.push_str(&format!(
        "- human decision: {}\n\n",
        human_decision_label(report)
    ));

    md.push_str("## Gates\n\n");
    if report.gates.is_empty() {
        md.push_str("_No gates recorded._\n\n");
    } else {
        md.push_str("| Gate | Level | State |\n|------|-------|-------|\n");
        for g in &report.gates {
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                gate_name_label(&g.name),
                requirement_level_label(&g.requirement_level),
                gate_state_label(&g.state)
            ));
        }
        md.push('\n');
    }

    md.push_str("## Findings\n\n");
    if report.findings.is_empty() {
        md.push_str("_No findings recorded._\n\n");
    } else {
        for f in &report.findings {
            md.push_str(&format!(
                "- **{}** ({}): {}\n",
                f.id.0,
                severity_label(&f.severity),
                f.title
            ));
        }
        md.push('\n');
    }

    md.push_str("## Coverage\n\n");
    md.push_str(&format!(
        "- audited areas: {}\n",
        report.coverage.audited_areas.len()
    ));
    md.push_str(&format!(
        "- skipped areas: {}\n",
        report.coverage.skipped_areas.len()
    ));
    md.push_str(&format!(
        "- blocked areas: {}\n",
        report.coverage.blocked_areas.len()
    ));
    md.push_str(&format!(
        "- executed mechanisms: {}\n",
        report.coverage.executed_mechanisms.len()
    ));
    if !report.coverage.known_limitations.is_empty() {
        md.push_str("\n### Known limitations\n\n");
        for lim in &report.coverage.known_limitations {
            md.push_str(&format!("- {lim}\n"));
        }
    }

    md
}

// --- wire-string labels (serde single source of truth) --------------------
//
// Labels come from serde_json serialization so the Markdown uses the exact
// frozen enum strings and no new vocabulary is introduced. Each helper takes a
// concrete `serde_json::to_value` call, so no explicit `serde::Serialize` bound
// (and thus no direct `serde` dependency) is needed.

macro_rules! wire_label {
    ($name:ident, $ty:path) => {
        fn $name(value: &$ty) -> String {
            serde_json::to_value(value)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default()
        }
    };
}

wire_label!(profile_id_label, fira_core::model::ProfileId);
wire_label!(gate_name_label, fira_core::model::GateName);
wire_label!(requirement_level_label, fira_core::model::RequirementLevel);
wire_label!(gate_state_label, fira_core::model::GateState);
wire_label!(severity_label, fira_core::model::Severity);
wire_label!(
    assessment_label,
    fira_core::model::TechnicalAssessmentResult
);
wire_label!(
    human_decision_label_state,
    fira_core::model::HumanDecisionFiraState
);

fn human_decision_label(report: &AuditReport) -> String {
    human_decision_label_state(&report.human_decision.state)
}
