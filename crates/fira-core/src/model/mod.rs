//! Shared model building blocks for the CORE domain types.
//!
//! The closed enums and id newtypes referenced across the S-schemas, centralized
//! so the per-schema types and parity tests agree on one frozen enum set.

pub mod enums;
pub mod ids;

pub use enums::*;
pub use ids::*;

use serde::{Deserialize, Serialize};

/// A field the schema fixes to the constant `true` (`const: true`): S6
/// `InferredInvariant.inferred` / `code_internal_only`, and S12
/// `VerificationMechanism.declared_by_project`.
///
/// Serializes as `true` and rejects any other value on deserialization, so the
/// contract-forbidden `false` state is unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "bool", into = "bool")]
pub struct AlwaysTrue;

impl TryFrom<bool> for AlwaysTrue {
    type Error = &'static str;

    fn try_from(value: bool) -> Result<Self, Self::Error> {
        if value {
            Ok(AlwaysTrue)
        } else {
            Err("field is fixed to true by the frozen schema (const: true)")
        }
    }
}

impl From<AlwaysTrue> for bool {
    fn from(_: AlwaysTrue) -> bool {
        true
    }
}
