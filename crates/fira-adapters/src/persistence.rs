//! Adapter-side persistence wiring for Task 11 (WS-1).
//!
//! Two pieces, both living in the ADAPTER per §9.5(B):
//! - [`AdapterAuditContextProvider`] implements CORE's §17
//!   [`fira_core::interfaces::AuditContextProvider`]. `persist_report` renders
//!   JSON + Markdown via PRESENTATION and hands the **bytes** to the RUNTIME
//!   [`WorkspaceSink`]; RUNTIME never renders and gains no `fira-presentation`
//!   dependency.
//! - [`resolve_default_base`]: the cross-platform per-user data-dir resolver,
//!   `std::env` only (no `dirs` crate), used when `--workspace` is absent
//!   (design §4.1).

use std::path::{Path, PathBuf};

use fira_core::interfaces::{AuditContextProvider, CapabilityError, PersistLocation};
use fira_core::report::AuditReport;
use fira_core::request::AuditRequest;

use fira_presentation::{parse_json, render_json, render_markdown};
use fira_runtime::{OverwritePolicy, WorkspaceError, WorkspaceSink};

/// The §17 provider: renders in the ADAPTER, persists bytes via RUNTIME. The
/// report is never mutated; a prior report (if any) is input-only (C12).
pub struct AdapterAuditContextProvider {
    request: AuditRequest,
    sink: WorkspaceSink,
    overwrite_policy: OverwritePolicy,
    prior: Option<AuditReport>,
}

impl AdapterAuditContextProvider {
    /// Construct a provider bound to a workspace base + project root. The sink
    /// rejects a base inside the project tree at construction (dual containment,
    /// AC-3). Defaults to [`OverwritePolicy::CreateNew`] so a fresh audit never
    /// clobbers an existing directory.
    pub fn new(
        request: AuditRequest,
        workspace_base: &Path,
        project_root: &Path,
    ) -> Result<Self, WorkspaceError> {
        let sink = WorkspaceSink::new(workspace_base, project_root)?;
        Ok(Self {
            request,
            sink,
            overwrite_policy: OverwritePolicy::CreateNew,
            prior: None,
        })
    }

    /// Set the overwrite policy (an intended re-persist uses
    /// [`OverwritePolicy::ReplaceSameId`]; the sink never infers intent from the
    /// id — design §2.4).
    pub fn with_overwrite_policy(mut self, policy: OverwritePolicy) -> Self {
        self.overwrite_policy = policy;
        self
    }

    /// Attach a prior report as historical context (§9.3). Input-only; its
    /// conclusions are never treated as current evidence (C12).
    pub fn with_prior(mut self, prior: Option<AuditReport>) -> Self {
        self.prior = prior;
        self
    }

    /// The canonicalized workspace base this provider persists under.
    pub fn workspace_base(&self) -> &Path {
        self.sink.workspace_base()
    }
}

impl AuditContextProvider for AdapterAuditContextProvider {
    fn get_request(&self) -> Result<AuditRequest, CapabilityError> {
        Ok(self.request.clone())
    }

    fn get_prior_report(&self) -> Result<Option<AuditReport>, CapabilityError> {
        Ok(self.prior.clone())
    }

    fn persist_report(&self, report: &AuditReport) -> Result<PersistLocation, CapabilityError> {
        // ADAPTER renders (PRESENTATION); RUNTIME publishes the bytes (§9.5 B).
        // The report is read-only here — never mutated.
        let json = render_json(report).into_bytes();
        let md = render_markdown(report).into_bytes();
        let loc = self
            .sink
            .persist_artifacts(
                &report.audit_id,
                &[("report.json", &json), ("report.md", &md)],
                self.overwrite_policy,
            )
            .map_err(map_workspace_err)?;
        Ok(PersistLocation(loc.to_string_lossy().into_owned()))
    }
}

/// Map a RUNTIME [`WorkspaceError`] onto CORE's [`CapabilityError`] (design §3.2).
/// Containment/collision violations are `NotPermitted`; I/O/resolution failures
/// are `Failed`. No `CapabilityError` variant is added.
fn map_workspace_err(e: WorkspaceError) -> CapabilityError {
    match e {
        WorkspaceError::WorkspaceInsideProject
        | WorkspaceError::OutsideWorkspace
        | WorkspaceError::InsideProjectTree
        | WorkspaceError::UnsafeAuditId(_)
        | WorkspaceError::AuditIdExists(_) => CapabilityError::NotPermitted,
        WorkspaceError::Unresolvable(msg) | WorkspaceError::WriteFailed(msg) => {
            CapabilityError::Failed(msg)
        }
    }
}

/// A read-only loader for a prior report (§9.3). Reads the given `report.json`
/// path, parses it, and **structurally accepts** it (it must deserialize into an
/// [`AuditReport`]). Returns the report as historical context; it is never
/// mutated and its conclusions are not treated as current evidence (C12). No
/// lifecycle logic (§14 is a separate task).
///
/// The caller must ensure the path is **outside the audited project tree**; this
/// loader is for prior audit-workspace artifacts, not project files.
pub fn load_prior_report(path: &Path) -> Result<Option<AuditReport>, CapabilityError> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let report = parse_json(&text)
                .map_err(|e| CapabilityError::Failed(format!("prior report parse failed: {e}")))?;
            Ok(Some(report))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(CapabilityError::Failed(format!(
            "prior report read failed: {e}"
        ))),
    }
}

/// The subdirectories appended to the resolved per-user data dir to form the
/// default workspace base (`<data-dir>/fira-guardian/fira-workspace`).
const APP_SUBDIR: &str = "fira-guardian";
const WORKSPACE_SUBDIR: &str = "fira-workspace";

/// The target OS family for default-base resolution. Selected with
/// `cfg!(target_os = ...)` in production; parameterized here so the candidate
/// order is unit-tested deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsFamily {
    /// Linux / BSD (XDG).
    Unix,
    /// macOS.
    MacOs,
    /// Windows.
    Windows,
}

impl OsFamily {
    /// The OS family of the current build target.
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            OsFamily::Windows
        } else if cfg!(target_os = "macos") {
            OsFamily::MacOs
        } else {
            OsFamily::Unix
        }
    }
}

/// Resolve the default workspace base for the current OS, using the process
/// environment (`std::env::var`). Returns `<data-dir>/fira-guardian/fira-workspace`
/// or an error asking for `--workspace` when no candidate qualifies.
pub fn resolve_default_base() -> Result<PathBuf, String> {
    resolve_default_base_with(OsFamily::current(), &|k| std::env::var(k).ok())
}

/// Testable core of [`resolve_default_base`]: an injected OS family + env lookup
/// so the candidate order and the absent/empty/relative fallbacks are unit-tested
/// without mutating the real environment (design §4.1).
///
/// A candidate env value is accepted only if it is **non-empty (after trimming)
/// and absolute**; otherwise it is skipped. If none qualifies, the run fails with
/// a clear error asking for `--workspace` — never a silent or relative location.
pub fn resolve_default_base_with(
    os: OsFamily,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<PathBuf, String> {
    let candidates: Vec<(&str, Option<&str>)> = match os {
        // (env var, optional literal suffix to append to its value)
        OsFamily::Unix => vec![("XDG_DATA_HOME", None), ("HOME", Some(".local/share"))],
        OsFamily::MacOs => vec![("HOME", Some("Library/Application Support"))],
        OsFamily::Windows => vec![
            ("LOCALAPPDATA", None),
            ("APPDATA", None),
            ("USERPROFILE", Some("AppData\\Local")),
        ],
    };

    for (var, suffix) in candidates {
        if let Some(dir) = qualified_env_dir(os, env, var) {
            let mut base = dir;
            if let Some(sfx) = suffix {
                base.push(sfx);
            }
            base.push(APP_SUBDIR);
            base.push(WORKSPACE_SUBDIR);
            return Ok(base);
        }
    }

    Err(format!(
        "could not resolve a default audit-workspace base from the environment; \
         re-run with --workspace <dir> (an absolute path outside the project tree). \
         (checked: {})",
        env_names(os).join(", ")
    ))
}

/// Return the env var's value as a [`PathBuf`] iff it is set, non-empty after
/// trimming, and absolute **for the simulated OS family**; otherwise `None`
/// (treated as unset). Absoluteness is judged by the target family — not the host
/// — so the candidate order is tested deterministically on any build host.
fn qualified_env_dir(
    os: OsFamily,
    env: &dyn Fn(&str) -> Option<String>,
    var: &str,
) -> Option<PathBuf> {
    let raw = env(var)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if is_absolute_for(os, trimmed) {
        Some(PathBuf::from(trimmed))
    } else {
        None
    }
}

/// Family-aware absoluteness: a Unix/macOS path is absolute iff it starts with
/// `/`; a Windows path is absolute iff it has a drive-letter root (`C:\`) or a
/// UNC prefix (`\\`). Host-independent so the simulated candidate order is
/// deterministic across build hosts.
fn is_absolute_for(os: OsFamily, s: &str) -> bool {
    match os {
        OsFamily::Unix | OsFamily::MacOs => s.starts_with('/'),
        OsFamily::Windows => {
            let b = s.as_bytes();
            let drive = b.len() >= 3
                && b[0].is_ascii_alphabetic()
                && b[1] == b':'
                && (b[2] == b'\\' || b[2] == b'/');
            let unc = s.starts_with("\\\\");
            drive || unc
        }
    }
}

/// The env var names consulted for a family (for the error message).
fn env_names(os: OsFamily) -> Vec<&'static str> {
    match os {
        OsFamily::Unix => vec!["XDG_DATA_HOME", "HOME"],
        OsFamily::MacOs => vec!["HOME"],
        OsFamily::Windows => vec!["LOCALAPPDATA", "APPDATA", "USERPROFILE"],
    }
}
