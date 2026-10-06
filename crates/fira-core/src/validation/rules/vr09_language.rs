//! VR9 — no "correct/safe" adjudication language in rationale/summary.
//!
//! A lexical scan over the adjudication-bearing text fields. Tokens are matched
//! as whole words, case-insensitively, so "correctness" (a neutral gate name)
//! and "unsafe" are not falsely flagged — only the adjectives "correct" and
//! "safe" themselves.

use serde_json::Value;

use crate::validation::{Violation, Vr};

/// Fixed adjudication token list (frozen contract §13 "no correct/safe language").
const PROHIBITED_TOKENS: [&str; 2] = ["correct", "safe"];

/// The report text locations that carry adjudication language.
fn scan_targets(report: &Value) -> Vec<(String, String)> {
    let mut targets = Vec::new();

    if let Some(s) = report.get("system_model_summary").and_then(Value::as_str) {
        targets.push(("system_model_summary".to_string(), s.to_string()));
    }

    if let Some(gates) = report.get("gates").and_then(Value::as_array) {
        for (i, gate) in gates.iter().enumerate() {
            if let Some(r) = gate.get("rationale").and_then(Value::as_str) {
                targets.push((format!("gates[{i}].rationale"), r.to_string()));
            }
        }
    }

    if let Some(findings) = report.get("findings").and_then(Value::as_array) {
        for (i, finding) in findings.iter().enumerate() {
            for field in ["title", "impact", "confidence_derivation"] {
                if let Some(v) = finding.get(field).and_then(Value::as_str) {
                    targets.push((format!("findings[{i}].{field}"), v.to_string()));
                }
            }
        }
    }

    targets
}

/// True if `haystack` contains `token` as a whole word (ASCII word boundaries),
/// case-insensitively.
fn contains_word(haystack: &str, token: &str) -> bool {
    let hay = haystack.to_ascii_lowercase();
    let tok = token.to_ascii_lowercase();
    let bytes = hay.as_bytes();
    let mut start = 0;
    while let Some(pos) = hay[start..].find(&tok) {
        let idx = start + pos;
        let before_ok = idx == 0 || !bytes[idx - 1].is_ascii_alphanumeric();
        let after = idx + tok.len();
        let after_ok = after >= bytes.len() || !bytes[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = idx + tok.len();
    }
    false
}

pub(crate) fn check(report: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    for (locator, text) in scan_targets(report) {
        for token in PROHIBITED_TOKENS {
            if contains_word(&text, token) {
                out.push(Violation::new(
                    Vr::Vr9,
                    locator.clone(),
                    format!("prohibited adjudication word `{token}`"),
                ));
            }
        }
    }
    out
}
