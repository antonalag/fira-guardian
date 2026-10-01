//! VR13 — no report/persistence path under `project_root`. *(correction 4)*
//!
//! From `schema-formalization.md` / frozen contract §19 WS-1/P-1: no report path
//! or runtime write target lies inside `project_root`; persistence targets the
//! external audit workspace only.
//!
//! ## Mechanical check (pure path containment)
//!
//! This is a pure predicate over strings: any persistence/report path in the
//! report that normalizes to a location under `ctx.project_root()` is a VR13
//! violation. CORE does **not** touch a real filesystem — the caller supplies
//! the `project_root` string via
//! [`ValidationContext`](crate::validation::ValidationContext), and no actual
//! write happens in Task 2. The report-side path candidate that exists in the
//! canonical `AuditReport` (S11) is `prior_report_ref`; the report has no other
//! persistence-target field in the frozen model, so only declared path fields
//! are checked (no new field is introduced).

use serde_json::Value;

use crate::validation::{ValidationContext, Violation, Vr};

/// Normalize a path string for prefix comparison: trim, drop a single trailing
/// `/`, and treat it as an opaque `/`-separated string (no FS access).
fn normalize(path: &str) -> String {
    let trimmed = path.trim();
    trimmed.strip_suffix('/').unwrap_or(trimmed).to_string()
}

/// True if `candidate` is the project root itself or a path strictly under it,
/// using `/`-boundary-aware prefix matching so `/repos/acme-lib-2` is NOT
/// considered under `/repos/acme-lib`.
fn is_under(root: &str, candidate: &str) -> bool {
    let root = normalize(root);
    let candidate = normalize(candidate);
    if root.is_empty() {
        return false;
    }
    if candidate == root {
        return true;
    }
    let mut prefix = root.clone();
    prefix.push('/');
    candidate.starts_with(&prefix)
}

/// Collect report path fields that represent a persistence/report target.
fn persistence_paths(report: &Value) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    if let Some(p) = report.get("prior_report_ref").and_then(Value::as_str) {
        paths.push(("prior_report_ref".to_string(), p.to_string()));
    }
    paths
}

pub(crate) fn check(report: &Value, ctx: &dyn ValidationContext) -> Vec<Violation> {
    let mut out = Vec::new();
    let root = ctx.project_root();
    for (locator, path) in persistence_paths(report) {
        if is_under(root, &path) {
            out.push(Violation::new(
                Vr::Vr13,
                locator,
                format!("persistence/report path `{path}` is under project_root `{root}` (WS-1/P-1: external workspace only)"),
            ));
        }
    }
    out
}
