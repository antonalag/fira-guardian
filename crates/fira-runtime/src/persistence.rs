//! External audit-workspace persistence (WS-1). Implemented in **Task 11**.
//!
//! [`WorkspaceSink`] is the external-workspace analogue of Task 10's project
//! [`crate::capability::OutputSink`], but with a **dual containment** rule — a
//! write must be **inside the workspace base** AND **outside the audited project
//! tree** (WS-1 / P-1 / VR13) — and a *bytes-in* API: it never renders and never
//! knows about `AuditReport`. Rendering happens in the ADAPTER (§9.5 B); RUNTIME
//! only receives `(file_name, bytes)` pairs and publishes them.
//!
//! Persistence is a **separate axis** from project-tree write capability
//! (ADR-002): this sink is bound to a `workspace_base` distinct from
//! `project_root`, rejects a base inside the project tree at construction, and
//! re-checks containment on every publish. No `WRITE_PROJECT` capability exists.
//!
//! # Publish strategy and the precise atomicity guarantee (design §2.2)
//!
//! The per-`audit_id` directory *is* the version/replace unit, so the pair is
//! published as a unit via a staging dir + directory rename:
//! - **First write of an `audit_id`** (final dir absent): a single
//!   `rename(staging → final)` — genuinely all-or-nothing; a crash leaves the
//!   final dir absent, never a half-pair.
//! - **Overwrite** ([`OverwritePolicy::ReplaceSameId`]): two whole-directory
//!   renames (move-old-aside to `.old-*`, then move-new-in from `.staging-*`).
//!   The window between them never exposes a half-pair at the canonical path —
//!   at most the canonical path is momentarily absent, with both complete
//!   versions recoverable from the sidecar dirs. This is **not** claimed as
//!   unqualified all-or-nothing.
//!
//! # TOCTOU honesty (design §2.3)
//!
//! Containment is check-then-use (canonicalize + verify before writing) and is
//! **not** race-proof against a privileged adversary mutating the filesystem
//! between the check and the write. Mitigation: all writes are confined beneath a
//! sink-created `.staging-*` directory and the final path is a pre-validated
//! rename target, not a string re-derived at write time. The residual window is a
//! documented, tested MVP limitation; `openat2(RESOLVE_BENEATH)` / `O_NOFOLLOW`
//! hardening is future work, out of MVP scope.
//!
//! # Durability vs rename atomicity (design §2.6)
//!
//! Rename atomicity governs *visibility ordering*, not crash durability. Files,
//! the staging dir, and `<base>` are fsynced before/around publish. Process-crash
//! correctness holds via rename ordering alone; system-crash durability holds
//! after fsync **on filesystems that honor it** — we do not claim durability on
//! mounts that ignore `fsync`, nor cross-filesystem atomicity (staging and final
//! are kept on the same filesystem so rename is a metadata move). `fsync`
//! failures surface as [`WorkspaceError::WriteFailed`], never ignored.

use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Why a workspace persist was rejected or failed (RUNTIME-local; **not** a
/// contract/schema type). The adapter maps these onto CORE's `CapabilityError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    /// The workspace base resolves inside the project tree (illegal config —
    /// persistence may never collapse into a project-tree write). Rejected at
    /// construction before any write path exists.
    WorkspaceInsideProject,
    /// An `audit_id` or file name is not a single safe path component
    /// (empty, contains a separator, is `.`/`..`, or is absolute).
    UnsafeAuditId(String),
    /// [`OverwritePolicy::CreateNew`] and `<base>/<audit_id>/` already exists.
    AuditIdExists(String),
    /// A resolved write path is not inside the workspace base.
    OutsideWorkspace,
    /// A resolved write path is on/inside the project tree.
    InsideProjectTree,
    /// A path could not be resolved to an absolute real path.
    Unresolvable(String),
    /// A write/fsync/rename failed (reported, never swallowed).
    WriteFailed(String),
}

impl core::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WorkspaceError::WorkspaceInsideProject => write!(
                f,
                "workspace base resolves inside the project tree (WS-1/P-1: persistence is a separate axis from project write)"
            ),
            WorkspaceError::UnsafeAuditId(what) => {
                write!(f, "unsafe audit-id/file name (not a single safe path component): {what}")
            }
            WorkspaceError::AuditIdExists(id) => {
                write!(f, "audit-id directory already exists (CreateNew policy): {id}")
            }
            WorkspaceError::OutsideWorkspace => {
                write!(f, "resolved write path is outside the workspace base")
            }
            WorkspaceError::InsideProjectTree => {
                write!(f, "resolved write path is inside the project tree (WS-1/P-1)")
            }
            WorkspaceError::Unresolvable(why) => write!(f, "workspace path unresolvable: {why}"),
            WorkspaceError::WriteFailed(why) => write!(f, "workspace write failed: {why}"),
        }
    }
}

impl std::error::Error for WorkspaceError {}

/// Whether an existing `<base>/<audit_id>/` may be replaced (design §2.4). The
/// sink never infers intent from the id; the caller chooses the policy, so the
/// common CLI path (`CreateNew`) cannot overwrite an existing directory by
/// accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverwritePolicy {
    /// Default: fail with [`WorkspaceError::AuditIdExists`] if the dir exists.
    #[default]
    CreateNew,
    /// Intended re-persist of the same id (move-aside + publish).
    ReplaceSameId,
}

/// Observes the durability (fsync) sequence (design §2.6). Production uses
/// [`RealFsync`], which fsyncs real file/dir handles; tests inject a recording
/// seam to assert the sequence is invoked without depending on a crash.
pub trait FsyncObserver {
    /// Fsync a file's contents (already written + flushed).
    fn fsync_file(&self, path: &Path) -> Result<(), WorkspaceError>;
    /// Fsync a directory's entries.
    fn fsync_dir(&self, path: &Path) -> Result<(), WorkspaceError>;
}

/// The production fsync observer: fsyncs real OS handles.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFsync;

impl FsyncObserver for RealFsync {
    fn fsync_file(&self, path: &Path) -> Result<(), WorkspaceError> {
        let f = File::open(path).map_err(|e| {
            WorkspaceError::WriteFailed(format!("open for fsync {}: {e}", path.display()))
        })?;
        f.sync_all()
            .map_err(|e| WorkspaceError::WriteFailed(format!("fsync file {}: {e}", path.display())))
    }

    fn fsync_dir(&self, path: &Path) -> Result<(), WorkspaceError> {
        // Opening a directory as a File is valid on Unix for fsync. On platforms
        // where it is not supported, a non-fatal failure is tolerated: directory
        // fsync is a durability refinement, and we do not overclaim durability on
        // filesystems/platforms that do not honor it.
        match File::open(path) {
            Ok(f) => match f.sync_all() {
                Ok(()) => Ok(()),
                Err(_) if cfg!(windows) => Ok(()),
                Err(e) => Err(WorkspaceError::WriteFailed(format!(
                    "fsync dir {}: {e}",
                    path.display()
                ))),
            },
            Err(_) if cfg!(windows) => Ok(()),
            Err(e) => Err(WorkspaceError::WriteFailed(format!(
                "open dir for fsync {}: {e}",
                path.display()
            ))),
        }
    }
}

/// Writes an audit's report pair under an audit-workspace base, enforcing the
/// dual-containment invariant and publishing via a staging-dir + directory
/// rename with fsync (design §2). Bytes-in: it never renders.
#[derive(Debug, Clone)]
pub struct WorkspaceSink<F: FsyncObserver = RealFsync> {
    workspace_base: PathBuf,
    project_root: PathBuf,
    fsync: F,
}

impl WorkspaceSink<RealFsync> {
    /// Bind to a workspace base + the project root. **Rejects** a base inside the
    /// project tree ([`WorkspaceError::WorkspaceInsideProject`]): persistence can
    /// never collapse into a project-tree write (AC-3). Both paths are
    /// canonicalized (they must exist).
    pub fn new(workspace_base: &Path, project_root: &Path) -> Result<Self, WorkspaceError> {
        Self::with_fsync(workspace_base, project_root, RealFsync)
    }
}

impl<F: FsyncObserver> WorkspaceSink<F> {
    /// Like [`WorkspaceSink::new`] but with an injected fsync observer (test seam
    /// for the durability sequence, design §2.6).
    pub fn with_fsync(
        workspace_base: &Path,
        project_root: &Path,
        fsync: F,
    ) -> Result<Self, WorkspaceError> {
        let workspace_base = fs::canonicalize(workspace_base)
            .map_err(|e| WorkspaceError::Unresolvable(format!("workspace_base: {e}")))?;
        let project_root = fs::canonicalize(project_root)
            .map_err(|e| WorkspaceError::Unresolvable(format!("project_root: {e}")))?;
        // The structural guarantee that the two axes never overlap: reject before
        // any write path exists. Checked both directions so neither can nest in
        // the other.
        if is_within(&project_root, &workspace_base) || is_within(&workspace_base, &project_root) {
            return Err(WorkspaceError::WorkspaceInsideProject);
        }
        Ok(WorkspaceSink {
            workspace_base,
            project_root,
            fsync,
        })
    }

    /// The canonicalized workspace base.
    pub fn workspace_base(&self) -> &Path {
        &self.workspace_base
    }

    /// Publish all `artifacts` as the directory `<base>/<audit_id>/` via a
    /// staging-dir + directory-rename publish (design §2.2) with fsync (§2.6),
    /// under the given overwrite `policy` (§2.4). Runs the crash-recovery sweep
    /// first (§2.5). Returns the canonical `<base>/<audit_id>/` directory.
    pub fn persist_artifacts(
        &self,
        audit_id: &str,
        artifacts: &[(&str, &[u8])],
        policy: OverwritePolicy,
    ) -> Result<PathBuf, WorkspaceError> {
        validate_component(audit_id)?;
        for (name, _) in artifacts {
            validate_component(name)?;
        }

        // §2.5: recover any interrupted overwrite for this id before publishing.
        self.recover(audit_id)?;

        let final_dir = self.workspace_base.join(audit_id);
        let exists = final_dir.exists();
        match policy {
            OverwritePolicy::CreateNew if exists => {
                return Err(WorkspaceError::AuditIdExists(audit_id.to_string()));
            }
            _ => {}
        }

        let nonce = nonce();

        // 1. Stage: write both files into a sibling `.staging-*` dir inside
        //    `<base>` (same filesystem ⇒ rename is a metadata move), fsyncing each
        //    file and the staging dir so their bytes are durable before publish.
        let staging = self
            .workspace_base
            .join(format!(".staging-{audit_id}-{nonce}"));
        if let Err(e) = self.stage(&staging, artifacts) {
            // Best-effort cleanup; the canonical path is untouched (§2.2 step 5).
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }

        // 2. Re-check containment of the resolved final target against the
        //    workspace base AND the project tree (dual containment, §2.1 step 3).
        if let Err(e) = self.check_target_within(&final_dir) {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }

        // 3. Publish.
        let old_dir = self.workspace_base.join(format!(".old-{audit_id}-{nonce}"));
        if exists {
            // Overwrite: move-old-aside (R1), move-new-in (R2), then drop old.
            if let Err(e) = rename(&final_dir, &old_dir) {
                let _ = fs::remove_dir_all(&staging);
                return Err(e);
            }
            if let Err(e) = rename(&staging, &final_dir) {
                // R2 failed: restore the previous version so the canonical path is
                // not left absent, then surface the failure.
                let _ = rename(&old_dir, &final_dir);
                let _ = fs::remove_dir_all(&staging);
                return Err(e);
            }
            // Canonical dir now holds the new complete pair; drop the old version.
            let _ = fs::remove_dir_all(&old_dir);
        } else {
            // First write: a single rename publishes the complete pair atomically.
            if let Err(e) = rename(&staging, &final_dir) {
                let _ = fs::remove_dir_all(&staging);
                return Err(e);
            }
        }

        // 4. Durability of the rename itself: fsync `<base>` so the directory-entry
        //    change that makes `<audit_id>/` point at the new data is durable.
        self.fsync.fsync_dir(&self.workspace_base)?;

        Ok(final_dir)
    }

    /// Write both artifacts into a fresh staging dir, fsyncing each file then the
    /// staging dir (design §2.2 step 3, §2.6).
    fn stage(&self, staging: &Path, artifacts: &[(&str, &[u8])]) -> Result<(), WorkspaceError> {
        fs::create_dir(staging).map_err(|e| {
            WorkspaceError::WriteFailed(format!("create staging {}: {e}", staging.display()))
        })?;
        for (name, bytes) in artifacts {
            let file_path = staging.join(name);
            fs::write(&file_path, bytes).map_err(|e| {
                WorkspaceError::WriteFailed(format!("write {}: {e}", file_path.display()))
            })?;
            self.fsync.fsync_file(&file_path)?;
        }
        self.fsync.fsync_dir(staging)?;
        Ok(())
    }

    /// Resolve the (created) target dir and require it to be inside the workspace
    /// base AND outside the project tree (dual containment, design §2.1 step 3).
    /// Resolves the nearest existing ancestor + lexical tail so a not-yet-created
    /// `<audit_id>/` can still be validated.
    fn check_target_within(&self, final_dir: &Path) -> Result<(), WorkspaceError> {
        let resolved = resolve_candidate(final_dir)?;
        if !is_within(&self.workspace_base, &resolved) {
            return Err(WorkspaceError::OutsideWorkspace);
        }
        if is_within(&self.project_root, &resolved) {
            return Err(WorkspaceError::InsideProjectTree);
        }
        Ok(())
    }

    /// Crash-recovery sweep (design §2.5): if `<audit_id>/` is absent but a
    /// complete sidecar survives from an interrupted overwrite, restore the
    /// canonical dir (prefer a committed `.staging-*`; else `.old-*`). Cleanup is
    /// restore-then-delete: a sidecar is removed only after the canonical dir is
    /// confirmed complete, so the only recoverable version is never deleted.
    fn recover(&self, audit_id: &str) -> Result<(), WorkspaceError> {
        let final_dir = self.workspace_base.join(audit_id);
        let staging_prefix = format!(".staging-{audit_id}-");
        let old_prefix = format!(".old-{audit_id}-");

        let mut staging_sidecars = Vec::new();
        let mut old_sidecars = Vec::new();
        let entries = match fs::read_dir(&self.workspace_base) {
            Ok(e) => e,
            Err(_) => return Ok(()), // base not readable yet ⇒ nothing to recover
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if name.starts_with(&staging_prefix) {
                staging_sidecars.push(path);
            } else if name.starts_with(&old_prefix) {
                old_sidecars.push(path);
            }
        }

        if staging_sidecars.is_empty() && old_sidecars.is_empty() {
            return Ok(());
        }

        let canonical_complete = final_dir.is_dir() && is_complete_pair(&final_dir);

        if !canonical_complete {
            // Restore: prefer a committed (complete) staging sidecar, else a
            // complete old sidecar. Restore-then-delete ordering.
            let restore_source = staging_sidecars
                .iter()
                .find(|p| is_complete_pair(p))
                .cloned()
                .or_else(|| old_sidecars.iter().find(|p| is_complete_pair(p)).cloned());

            if let Some(src) = restore_source {
                if final_dir.exists() {
                    // An incomplete canonical dir may remain; move it aside and
                    // drop it only after the restore succeeds.
                    let salvage = self
                        .workspace_base
                        .join(format!(".old-{audit_id}-{}", nonce()));
                    rename(&final_dir, &salvage)?;
                    rename(&src, &final_dir)?;
                    let _ = fs::remove_dir_all(&salvage);
                } else {
                    rename(&src, &final_dir)?;
                }
            } else {
                // No complete sidecar to restore from: leave sidecars in place
                // (never delete the only potentially recoverable data) and return.
                return Ok(());
            }
        }

        // Canonical dir is now confirmed complete: safe to delete remaining
        // sidecars (an incomplete `.staging-*` was never a committed version; a
        // `.old-*` is superseded by the confirmed canonical pair).
        for p in staging_sidecars.iter().chain(old_sidecars.iter()) {
            if p.exists() {
                let _ = fs::remove_dir_all(p);
            }
        }
        Ok(())
    }
}

/// A directory holds a "complete pair" iff both `report.json` and `report.md`
/// exist and are non-empty (design §2.5 completeness check).
fn is_complete_pair(dir: &Path) -> bool {
    ["report.json", "report.md"].iter().all(|f| {
        fs::metadata(dir.join(f))
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false)
    })
}

/// Validate a single safe path component: non-empty, no separators, not `.`/`..`,
/// not absolute (design §2.1 step 2). Never sanitized into a path.
fn validate_component(name: &str) -> Result<(), WorkspaceError> {
    // Reject empty and any interior NUL (invalid in a filesystem path and a
    // classic smuggling vector) before the component check.
    if name.is_empty() || name.contains('\0') {
        return Err(WorkspaceError::UnsafeAuditId(name.to_string()));
    }
    let p = Path::new(name);
    let mut comps = p.components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(c)), None) if c == std::ffi::OsStr::new(name) => Ok(()),
        _ => Err(WorkspaceError::UnsafeAuditId(name.to_string())),
    }
}

/// A process-unique nonce from `std::time` + a monotonic counter (design §2.4);
/// no new crate, no network. Distinct audits get distinct ids by construction
/// when the adapter composes `<release-target>-<nonce>`.
fn nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{seq:x}")
}

/// Rename a directory, mapping failure to [`WorkspaceError::WriteFailed`].
fn rename(from: &Path, to: &Path) -> Result<(), WorkspaceError> {
    fs::rename(from, to).map_err(|e| {
        WorkspaceError::WriteFailed(format!(
            "rename {} -> {}: {e}",
            from.display(),
            to.display()
        ))
    })
}

/// Resolve a path to an absolute real candidate without requiring it to exist:
/// canonicalize the nearest existing ancestor (resolving symlinks), then
/// re-append the remaining lexical components. Mirrors Task 10's technique.
fn resolve_candidate(requested: &Path) -> Result<PathBuf, WorkspaceError> {
    let abs = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| WorkspaceError::Unresolvable(format!("cwd: {e}")))?
            .join(requested)
    };
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
    let base = fs::canonicalize(existing)
        .map_err(|e| WorkspaceError::Unresolvable(format!("{}: {e}", existing.display())))?;
    let mut candidate = base;
    for comp in tail.iter().rev() {
        if let Component::Normal(name) = comp {
            candidate.push(name);
        }
    }
    Ok(candidate)
}

/// True if `candidate` is `root` or a descendant, compared component-wise on
/// (assumed canonicalized) absolute paths. Component-wise avoids the
/// `/repo` vs `/repo-2` string-prefix false match (mirrors Task 10
/// `capability.rs`).
fn is_within(root: &Path, candidate: &Path) -> bool {
    let r: Vec<Component> = root.components().collect();
    let c: Vec<Component> = candidate.components().collect();
    if c.len() < r.len() {
        return false;
    }
    r.iter().zip(c.iter()).all(|(a, b)| a == b)
}
