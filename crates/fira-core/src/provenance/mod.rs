//! Report provenance (report-provenance-slice): the pure CORE side of the S11
//! `determinism_inputs_hash` and `created_at` fields.
//!
//! Everything here is pure computation: no clock, filesystem, process or
//! network access. The algorithm (`fira-dih/1`) is specified in
//! `.kiro/specs/report-provenance-slice/design.md` §3.
//!
//! The hash covers exactly the inputs of
//! [`compute_assessment`](crate::verdict::compute_assessment): the gates, the
//! findings and the expectation-violation set. [`assess_with_inputs_hash`] is
//! the single production entry point that feeds the same values to both.

mod canonical;
mod timestamp;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::finding::Finding;
use crate::gate::Gate;
use crate::model::GateName;
use crate::verdict::{compute_assessment, ExpectationViolations, RecomputedAssessment};

/// Scheme identifier embedded in the hashed envelope. A deliberate change to
/// the canonical form requires a new scheme id (design §3.7).
pub const DETERMINISM_HASH_SCHEME: &str = "fira-dih/1";

/// Prefix of every `determinism_inputs_hash` value (design §3.5).
pub const DETERMINISM_HASH_PREFIX: &str = "sha256:";

/// Errors of the provenance computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceError {
    /// A hashed value could not be converted to the canonical data model
    /// (serde failure, or a non-integer number — design §3.4 N-1).
    Canonicalization(String),
}

impl core::fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProvenanceError::Canonicalization(e) => write!(f, "canonicalization failed: {e}"),
        }
    }
}

impl std::error::Error for ProvenanceError {}

impl From<canonical::CanonicalError> for ProvenanceError {
    fn from(e: canonical::CanonicalError) -> Self {
        ProvenanceError::Canonicalization(e.0)
    }
}

/// Hash of the verdict-engine inputs (design §3), as `sha256:<64 lowercase hex>`.
///
/// Normalizes `expectation_violations` (design §3.3) internally, so a caller
/// recomputing from a persisted report passes `&[]` (requirements R3.2).
pub fn determinism_inputs_hash(
    gates: &[Gate],
    findings: &[Finding],
    expectation_violations: &[GateName],
) -> Result<String, ProvenanceError> {
    let normalized = normalize_expectations(expectation_violations)?;
    hash_normalized(gates, findings, &normalized)
}

/// The single production entry point (requirements R2.3): normalizes the
/// expectation set once, passes the same gates, findings and normalized set to
/// the existing, unchanged [`compute_assessment`] and to the hash, and returns
/// both. Contains no verdict logic.
pub fn assess_with_inputs_hash(
    gates: &[Gate],
    findings: &[Finding],
    expectation_violations: &[GateName],
) -> Result<(RecomputedAssessment, String), ProvenanceError> {
    let normalized = normalize_expectations(expectation_violations)?;
    let assessment = compute_assessment(
        gates,
        findings,
        &ExpectationViolations::from_gates(normalized.clone()),
    );
    let hash = hash_normalized(gates, findings, &normalized)?;
    Ok((assessment, hash))
}

/// Lowercase-hex SHA-256 of `bytes`, via `sha2::Sha256`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        out.push(HEX[usize::from(b >> 4)] as char);
        out.push(HEX[usize::from(b & 0x0f)] as char);
    }
    out
}

/// `YYYY-MM-DDTHH:MM:SSZ` (UTC, second precision) for a Unix-seconds value;
/// `None` if the year would exceed 9999 (design §5). Pure: the caller (the
/// adapter) supplies the value, CORE never reads a clock.
pub fn format_utc_timestamp(unix_seconds: u64) -> Option<String> {
    timestamp::format_utc_timestamp(unix_seconds)
}

/// Design §3.3: sort by the frozen wire string (UTF-8 bytes), then remove exact
/// duplicates. The engine only queries membership, so this never changes its
/// evaluation.
fn normalize_expectations(gates: &[GateName]) -> Result<Vec<GateName>, ProvenanceError> {
    let mut keyed: Vec<(String, GateName)> = gates
        .iter()
        .map(|g| gate_wire(g).map(|w| (w, *g)))
        .collect::<Result<_, _>>()?;
    keyed.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    keyed.dedup_by(|a, b| a.0 == b.0);
    Ok(keyed.into_iter().map(|(_, g)| g).collect())
}

/// Build the design §3.2 envelope from already-normalized expectations and
/// hash its canonical bytes (design §3.4, §3.5).
fn hash_normalized(
    gates: &[Gate],
    findings: &[Finding],
    normalized: &[GateName],
) -> Result<String, ProvenanceError> {
    let bytes = envelope_bytes(gates, findings, normalized)?;
    Ok(format!("{DETERMINISM_HASH_PREFIX}{}", sha256_hex(&bytes)))
}

/// Canonical bytes of the design §3.2 envelope.
fn envelope_bytes(
    gates: &[Gate],
    findings: &[Finding],
    normalized: &[GateName],
) -> Result<Vec<u8>, ProvenanceError> {
    let expectation_strings = normalized
        .iter()
        .map(|g| gate_wire(g).map(Value::String))
        .collect::<Result<Vec<_>, _>>()?;

    let mut envelope = Map::new();
    envelope.insert(
        "scheme".to_string(),
        Value::String(DETERMINISM_HASH_SCHEME.to_string()),
    );
    envelope.insert("gates".to_string(), to_tree(gates)?);
    envelope.insert("findings".to_string(), to_tree(findings)?);
    envelope.insert(
        "expectation_violations".to_string(),
        Value::Array(expectation_strings),
    );

    Ok(canonical::to_canonical_bytes(&Value::Object(envelope))?)
}

/// Design §3.1: the JSON data-model tree via the CORE serde derives.
fn to_tree<T: serde::Serialize + ?Sized>(value: &T) -> Result<Value, ProvenanceError> {
    serde_json::to_value(value).map_err(|e| ProvenanceError::Canonicalization(e.to_string()))
}

/// The frozen wire string of a gate name (its serde representation).
fn gate_wire(gate: &GateName) -> Result<String, ProvenanceError> {
    match to_tree(gate)? {
        Value::String(s) => Ok(s),
        other => Err(ProvenanceError::Canonicalization(format!(
            "gate name did not serialize to a string: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        canonical::CanonicalError, envelope_bytes, normalize_expectations, ProvenanceError,
    };
    use crate::model::GateName;

    /// AC-2: the empty envelope's canonical bytes, byte for byte.
    #[test]
    fn empty_envelope_bytes() {
        assert_eq!(
            envelope_bytes(&[], &[], &[]).unwrap(),
            br#"{"expectation_violations":[],"findings":[],"gates":[],"scheme":"fira-dih/1"}"#
                .to_vec()
        );
    }

    /// Design §3.3: sorted by wire string, duplicates removed.
    #[test]
    fn normalization_sorts_and_dedups() {
        let n = normalize_expectations(&[
            GateName::Tests,
            GateName::FailureRecovery,
            GateName::Build,
            GateName::Tests,
        ])
        .unwrap();
        assert_eq!(
            n,
            vec![GateName::Build, GateName::FailureRecovery, GateName::Tests]
        );
    }

    #[test]
    fn canonical_error_maps_to_canonicalization() {
        let e: ProvenanceError = CanonicalError("boom".to_string()).into();
        assert_eq!(e, ProvenanceError::Canonicalization("boom".to_string()));
    }
}
