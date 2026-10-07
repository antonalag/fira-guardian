//! Capability enforcement over the audited project tree (CAP-1).
//!
//! - [`FsRepositoryReader`]: the READ capability. It can only read; there is no
//!   write/create/delete method, so no project-tree write is representable.
//! - [`OutputSink`]: the single CLI output-to-file sink. It enforces WS-1/P-1 by
//!   rejecting any destination that resolves onto or inside `project_root`
//!   (including `..` traversal and symlink re-entry). RUNTIME owns this
//!   rejection — not the CLI.

use std::fs;
use std::path::{Component, Path, PathBuf};

use fira_core::interfaces::{CapabilityError, LineRange, RepositoryReader};
use fira_core::model::Depth;

/// Reads the audited project tree, confined to a canonicalized `project_root`
/// (READ only; CAP-1). No method writes, creates, or deletes.
#[derive(Debug, Clone)]
pub struct FsRepositoryReader {
    root: PathBuf,
}

impl FsRepositoryReader {
    /// Build a reader confined to `project_root`. The root is canonicalized (it
    /// must exist); a non-resolvable root is a capability failure.
    pub fn new(project_root: impl AsRef<Path>) -> Result<Self, CapabilityError> {
        let root = fs::canonicalize(project_root.as_ref())
            .map_err(|e| CapabilityError::Failed(format!("project_root unresolvable: {e}")))?;
        Ok(FsRepositoryReader { root })
    }

    /// The canonicalized project root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a caller-supplied path (absolute or relative to root) to a real
    /// path confined to the project tree, or an error if it escapes.
    fn resolve_within(&self, path: &str) -> Result<PathBuf, CapabilityError> {
        let requested = Path::new(path);
        let joined = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            self.root.join(requested)
        };
        let real = fs::canonicalize(&joined)
            .map_err(|_| CapabilityError::NotFound(path.to_string()))?;
        if is_within(&self.root, &real) {
            Ok(real)
        } else {
            Err(CapabilityError::NotPermitted)
        }
    }
}

impl RepositoryReader for FsRepositoryReader {
    fn read_file(&self, path: &str, range: Option<LineRange>) -> Result<String, CapabilityError> {
        let real = self.resolve_within(path)?;
        if !real.is_file() {
            return Err(CapabilityError::NotFound(path.to_string()));
        }
        let content = fs::read_to_string(&real)
            .map_err(|e| CapabilityError::Failed(format!("read failed: {e}")))?;
        match range {
            None => Ok(content),
            Some(LineRange { start, end }) => {
                if start == 0 || end < start {
                    return Err(CapabilityError::Failed(format!(
                        "invalid line range {start}..{end}"
                    )));
                }
                // 1-based inclusive range.
                let selected: Vec<&str> = content
                    .lines()
                    .skip((start - 1) as usize)
                    .take((end - start + 1) as usize)
                    .collect();
                Ok(selected.join("\n"))
            }
        }
    }

    fn list_structure(
        &self,
        path: &str,
        depth: Option<Depth>,
    ) -> Result<Vec<String>, CapabilityError> {
        let base = self.resolve_within(path)?;
        if !base.is_dir() {
            return Err(CapabilityError::NotFound(path.to_string()));
        }
        // Depth: SHALLOW = immediate children only; STANDARD = 2 levels; DEEP =
        // unbounded. Default (None) = immediate children (conservative).
        let max_depth = match depth {
            None | Some(Depth::Shallow) => 1usize,
            Some(Depth::Standard) => 2,
            Some(Depth::Deep) => usize::MAX,
        };
        let mut out = Vec::new();
        list_recursive(&base, &base, 1, max_depth, &mut out)?;
        out.sort();
        Ok(out)
    }

    fn resolve_ref(&self, locator: &str) -> Result<bool, CapabilityError> {
        // A locator may carry a `file:line-range` suffix; only the path part is
        // resolved for existence. Confinement still applies.
        let path_part = locator.split(':').next().unwrap_or(locator);
        match self.resolve_within(path_part) {
            Ok(_) => Ok(true),
            Err(CapabilityError::NotFound(_)) => Ok(false),
            Err(other) => Err(other),
        }
    }
}

fn list_recursive(
    base: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<String>,
) -> Result<(), CapabilityError> {
    let entries =
        fs::read_dir(dir).map_err(|e| CapabilityError::Failed(format!("list failed: {e}")))?;
    for entry in entries {
        let entry = entry.map_err(|e| CapabilityError::Failed(format!("list failed: {e}")))?;
        let p = entry.path();
        if let Ok(rel) = p.strip_prefix(base) {
            out.push(rel.to_string_lossy().into_owned());
        }
        if p.is_dir() && depth < max_depth {
            list_recursive(base, &p, depth + 1, max_depth, out)?;
        }
    }
    Ok(())
}

/// Why an output destination was rejected (WS-1/P-1). See [`OutputSink::write`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputPathError {
    /// Destination is `project_root` itself.
    IsProjectRoot,
    /// Destination is a descendant of `project_root`.
    InsideProjectRoot,
    /// Destination escapes/re-enters the tree via `..` or symlink resolution.
    EscapesViaTraversalOrSymlink,
    /// The destination could not be resolved to an absolute real path.
    Unresolvable(String),
    /// The validated write itself failed.
    WriteFailed(String),
}

impl core::fmt::Display for OutputPathError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            OutputPathError::IsProjectRoot => {
                write!(f, "output path is the project root (WS-1/P-1: project tree is non-writable)")
            }
            OutputPathError::InsideProjectRoot => {
                write!(f, "output path is inside the project tree (WS-1/P-1)")
            }
            OutputPathError::EscapesViaTraversalOrSymlink => {
                write!(f, "output path escapes/re-enters the project tree via traversal or symlink (WS-1/P-1)")
            }
            OutputPathError::Unresolvable(why) => write!(f, "output path unresolvable: {why}"),
            OutputPathError::WriteFailed(why) => write!(f, "output write failed: {why}"),
        }
    }
}

/// The single CLI output-to-file sink. Validates a destination against the
/// project tree (WS-1/P-1) and only then writes. Never writes onto or inside
/// `project_root`.
#[derive(Debug, Clone)]
pub struct OutputSink {
    root: PathBuf,
}

impl OutputSink {
    /// Build a sink bound to a project root (canonicalized; must exist).
    pub fn new(project_root: impl AsRef<Path>) -> Result<Self, OutputPathError> {
        let root = fs::canonicalize(project_root.as_ref())
            .map_err(|e| OutputPathError::Unresolvable(format!("project_root: {e}")))?;
        Ok(OutputSink { root })
    }

    /// Resolve a requested output path to an absolute real candidate without
    /// requiring the file to pre-exist: canonicalize the nearest existing
    /// ancestor (resolving symlinks), then re-append the remaining components.
    fn resolve_candidate(&self, requested: &Path) -> Result<PathBuf, OutputPathError> {
        let abs = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| OutputPathError::Unresolvable(format!("cwd: {e}")))?
                .join(requested)
        };

        // Split into the nearest existing ancestor + the non-existent tail.
        let mut existing = abs.as_path();
        let mut tail: Vec<Component> = Vec::new();
        loop {
            if existing.exists() {
                break;
            }
            match existing.parent() {
                Some(parent) => {
                    if let Some(name) = existing.file_name() {
                        tail.push(Component::Normal(name));
                    }
                    existing = parent;
                }
                None => break,
            }
        }
        let base = fs::canonicalize(existing).map_err(|e| {
            OutputPathError::Unresolvable(format!("{}: {e}", existing.display()))
        })?;
        let mut candidate = base;
        for comp in tail.iter().rev() {
            if let Component::Normal(name) = comp {
                candidate.push(name);
            }
        }
        Ok(candidate)
    }

    /// Validate + write. Rejects project_root itself, any descendant, and any
    /// `..`/symlink escape that re-enters the tree; writes only an
    /// outside-the-tree destination.
    pub fn write(&self, requested: &Path, bytes: &[u8]) -> Result<PathBuf, OutputPathError> {
        let candidate = self.resolve_candidate(requested)?;

        if candidate == self.root {
            return Err(OutputPathError::IsProjectRoot);
        }
        if is_within(&self.root, &candidate) {
            // The requested path contained traversal/symlink components if its
            // lexical form did not already sit inside root; distinguish for a
            // clearer diagnostic.
            if lexically_inside(&self.root, requested) {
                return Err(OutputPathError::InsideProjectRoot);
            }
            return Err(OutputPathError::EscapesViaTraversalOrSymlink);
        }

        fs::write(&candidate, bytes)
            .map_err(|e| OutputPathError::WriteFailed(e.to_string()))?;
        Ok(candidate)
    }
}

/// True if `candidate` is `root` or a descendant, compared component-wise on
/// (assumed canonicalized) absolute paths. Component-wise avoids the
/// `/repo` vs `/repo-2` string-prefix false match.
fn is_within(root: &Path, candidate: &Path) -> bool {
    let r: Vec<Component> = root.components().collect();
    let c: Vec<Component> = candidate.components().collect();
    if c.len() < r.len() {
        return false;
    }
    r.iter().zip(c.iter()).all(|(a, b)| a == b)
}

/// A best-effort lexical check (no FS access) of whether a requested path, as
/// written, already sits inside root — used only to pick between the
/// `InsideProjectRoot` and `EscapesViaTraversalOrSymlink` diagnostics.
fn lexically_inside(root: &Path, requested: &Path) -> bool {
    let has_parent_traversal = requested
        .components()
        .any(|c| matches!(c, Component::ParentDir));
    if has_parent_traversal {
        return false;
    }
    if requested.is_absolute() {
        is_within(root, requested)
    } else {
        // Relative with no `..`: inside iff joining onto root stays prefixed,
        // which it always does lexically.
        true
    }
}
