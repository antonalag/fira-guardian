//! Shared test helpers: compile the bundled S2 JSON Schema (Draft 2020-12) with
//! a resolver over the full `schemas/` set so cross-references
//! (`urn:fira:schema:...`) resolve offline (TRANSPORT-1: local process only).

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jsonschema::{SchemaResolver, SchemaResolverError};
use serde_json::Value;
use url::Url;

/// Workspace root = two levels above `crates/fira-policy`.
pub fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root two levels above fira-policy manifest dir")
        .to_path_buf()
}

pub fn schemas_dir() -> PathBuf {
    workspace_root().join("schemas")
}

/// The twelve S-schemas, loaded so cross-references between them resolve.
pub const SCHEMA_FILES: [&str; 12] = [
    "s01-audit-request.schema.json",
    "s02-audit-profile.schema.json",
    "s03-epistemic-state.schema.json",
    "s04-evidence-support-mapping.schema.json",
    "s05-execution-result.schema.json",
    "s06-requirement-family.schema.json",
    "s07-finding.schema.json",
    "s08-gate.schema.json",
    "s09-coverage-statement.schema.json",
    "s10-assessment-decision.schema.json",
    "s11-audit-report.schema.json",
    "s12-verification-mechanism.schema.json",
];

/// Load every bundled schema (the 12 S-schemas plus `_defs/common`), keyed by
/// `$id`.
fn load_all_schemas_by_id() -> HashMap<String, Arc<Value>> {
    let mut map = HashMap::new();
    let dir = schemas_dir();

    let mut files: Vec<PathBuf> = SCHEMA_FILES.iter().map(|f| dir.join(f)).collect();
    files.push(dir.join("_defs").join("common.schema.json"));

    for path in files {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let value: Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
        let id = value
            .get("$id")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{} has no $id", path.display()))
            .to_string();
        map.insert(id, Arc::new(value));
    }
    map
}

struct BundledResolver {
    by_id: HashMap<String, Arc<Value>>,
}

impl BundledResolver {
    fn new() -> Self {
        BundledResolver {
            by_id: load_all_schemas_by_id(),
        }
    }
}

impl SchemaResolver for BundledResolver {
    fn resolve(
        &self,
        _root_schema: &Value,
        url: &Url,
        original_reference: &str,
    ) -> Result<Arc<Value>, SchemaResolverError> {
        let key = url.as_str().trim_end_matches('#');
        if let Some(doc) = self.by_id.get(key) {
            return Ok(Arc::clone(doc));
        }
        let orig = original_reference.trim_end_matches('#');
        if let Some(doc) = self.by_id.get(orig) {
            return Ok(Arc::clone(doc));
        }
        Err(SchemaResolverError::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("unresolved schema reference: url=`{url}` original=`{original_reference}`"),
        )))
    }
}

/// Compile the bundled S2 AuditProfile schema with cross-refs resolved.
pub fn compile_s2_schema() -> jsonschema::JSONSchema {
    let dir = schemas_dir();
    let path = dir.join("s02-audit-profile.schema.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let schema: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
    jsonschema::JSONSchema::options()
        .with_resolver(BundledResolver::new())
        .compile(&schema)
        .expect("S2 schema compiles")
}
