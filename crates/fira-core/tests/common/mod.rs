//! Shared test helpers for Task 2 schema + VR tests.
//!
//! Loads the bundled `schemas/` JSON Schema files (Draft 2020-12) and provides a
//! [`SchemaResolver`] that resolves the `urn:fira:schema:...` cross-references
//! between them, so validation runs fully offline (TRANSPORT-1: local process
//! only).

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jsonschema::{SchemaResolver, SchemaResolverError};
use serde_json::Value;
use url::Url;

/// Workspace root = two levels above `crates/fira-core`.
pub fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root two levels above fira-core manifest dir")
        .to_path_buf()
}

pub fn schemas_dir() -> PathBuf {
    workspace_root().join("schemas")
}

/// The twelve schema file names S1..S12, in order.
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

/// Load a JSON file relative to the workspace root.
pub fn load_json(rel: &str) -> Value {
    let path = workspace_root().join(rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()))
}

/// Load every bundled schema (the 12 S-schemas plus `_defs/common`), keyed by
/// its `$id`.
pub fn load_all_schemas_by_id() -> HashMap<String, Arc<Value>> {
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

/// Resolver over the bundled schema set: maps a `urn:fira:schema:...` URL back to
/// the loaded schema document with that `$id`.
pub struct BundledResolver {
    by_id: HashMap<String, Arc<Value>>,
}

impl BundledResolver {
    pub fn new() -> Self {
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
        // Cross-references use the full URN $id as the reference. jsonschema
        // hands us a parsed Url; for urn: scheme the whole string is the id.
        let key = url.as_str().trim_end_matches('#');
        if let Some(doc) = self.by_id.get(key) {
            return Ok(Arc::clone(doc));
        }
        // Fall back to matching the original reference string verbatim.
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

/// Compile a schema `Value` into a validator with the bundled resolver wired in.
pub fn compile(schema: &Value) -> jsonschema::JSONSchema {
    jsonschema::JSONSchema::options()
        .with_resolver(BundledResolver::new())
        .compile(schema)
        .expect("schema compiles")
}

/// Load and compile a named schema file from `schemas/`.
pub fn compile_schema_file(file: &str) -> jsonschema::JSONSchema {
    let schema = load_json(&format!("schemas/{file}"));
    compile(&schema)
}
