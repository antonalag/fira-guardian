//! PRESENTATION tests: JSON round-trips the report; Markdown is a non-mutating,
//! non-adjudicating projection.

use std::path::PathBuf;

use fira_core::report::AuditReport;
use fira_presentation::{render_json, render_markdown};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

fn golden_report() -> AuditReport {
    let path = workspace_root().join("schemas/examples/valid/s11-audit-report.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("golden report deserializes into AuditReport")
}

/// `render_json` round-trips: parsing the output yields an equal report.
#[test]
fn json_round_trips() {
    let report = golden_report();
    let rendered = render_json(&report);
    let reparsed: AuditReport =
        serde_json::from_str(&rendered).expect("rendered JSON reparses");
    assert_eq!(report, reparsed);
}

/// `render_markdown` does not mutate the report (compare before/after by value).
#[test]
fn markdown_does_not_mutate() {
    let report = golden_report();
    let before = report.clone();
    let _ = render_markdown(&report);
    assert_eq!(report, before);
}

/// `render_markdown` contains the expected sections.
#[test]
fn markdown_has_expected_sections() {
    let md = render_markdown(&golden_report());
    for section in [
        "# FIRA Guardian Audit Report",
        "## Technical assessment",
        "## Gates",
        "## Findings",
        "## Coverage",
    ] {
        assert!(md.contains(section), "missing section: {section}");
    }
}

/// `render_markdown` injects no adjudication ("correct"/"safe") vocabulary of
/// its own (VR9 spirit). We check the rendered scaffolding tokens; any such word
/// present would have to come from recorded report text, not the renderer.
#[test]
fn markdown_scaffolding_has_no_adjudication_words() {
    // Render a report whose own text is adjudication-free, so any "correct"/
    // "safe" token in the output could only come from the renderer's scaffolding.
    let mut report = golden_report();
    report.system_model_summary = "neutral summary".to_string();
    for g in &mut report.gates {
        g.rationale = "neutral".to_string();
    }
    for f in &mut report.findings {
        f.title = "neutral".to_string();
        f.impact = "neutral".to_string();
        f.confidence_derivation = "neutral".to_string();
    }
    let md = render_markdown(&report).to_lowercase();
    // Whole-word-ish check for the two prohibited adjudication adjectives.
    for word in [" correct ", " correct.", " safe ", " safe."] {
        assert!(!md.contains(word), "renderer scaffolding contains adjudication word via `{word}`");
    }
}
