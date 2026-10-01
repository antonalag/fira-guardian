//! Shared identifier newtypes for the CORE domain model.
//!
//! Thin `#[serde(transparent)]` wrappers over `String` that give type-level
//! distinction between kinds of references without changing the wire form. Task 4
//! does **not** enforce format patterns (e.g. the RR-NNN form of a finding id) in
//! the constructor: the JSON Schemas (Task 2) already own pattern enforcement.
//! Keeping these plain preserves exact round-trip and avoids introducing a new
//! validation semantic in Task 4.

use serde::{Deserialize, Serialize};

/// A generic non-empty identifier used for cross-references (ref_id, command_id,
/// finding references, etc.). Analogue of the schema `IdRef`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdRef(pub String);

/// A stable finding id (RR-NNN form; the pattern is enforced by the S7 schema).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FindingId(pub String);

impl From<&str> for IdRef {
    fn from(s: &str) -> Self {
        IdRef(s.to_string())
    }
}

impl From<&str> for FindingId {
    fn from(s: &str) -> Self {
        FindingId(s.to_string())
    }
}
