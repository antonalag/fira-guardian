//! Shared model building blocks for the CORE domain types.
//!
//! Rust analogue of `schemas/_defs/common.schema.json`: the closed enums and id
//! newtypes referenced across S1/S3/S6/S7/S8/S9/S10/S11. Centralizing them gives
//! a single source of truth so the per-schema types and the parity tests agree
//! on the frozen enum sets.
//!
//! Task 4 scope: data only — no rule, rubric, or verdict logic.

pub mod enums;
pub mod ids;

pub use enums::*;
pub use ids::*;

use serde::{Deserialize, Serialize};

/// A field that the frozen schema fixes to the constant boolean `true`
/// (schema `const: true`): S6 `InferredInvariant.inferred` /
/// `InferredInvariant.code_internal_only`, and S12
/// `VerificationMechanism.declared_by_project`.
///
/// Serializes as `true`; deserialization rejects any value other than `true`.
/// This mirrors the schema's `const: true` exactly — it introduces no new rule,
/// it reflects an already-approved structural constant into the type system so a
/// `false` state (which the contract forbids) is unrepresentable.
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
