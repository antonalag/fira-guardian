// Structured error taxonomy for the Kiro adapter (requirements §2.4 / AC-6).
//
// Two families:
//  - CLI-mirrored categories: preserve the Rust CLI's `AdapterError` meaning so a
//    host sees the same failure the auditor reported.
//  - Adapter-local transport categories: discovery / version / spawn / output
//    problems that occur on the TypeScript side of the boundary.
//
// Every category fails closed (an error is thrown/rejected; nothing is silently
// defaulted).

export type KiroErrorKind =
  // --- CLI-mirrored (from the auditor's AdapterError Display + exit code) ---
  | "Capability"
  | "UndeterminedNeedsProfile"
  | "NeedsExplicitConfirmation"
  | "Output"
  | "WorkspaceUnresolved"
  | "Persist"
  | "AuditFailed" // a non-zero exit whose stderr did not match a known category
  // --- adapter-local transport ---
  | "BinaryNotFound"
  | "VersionMismatch"
  | "SpawnFailed"
  | "MalformedOutput"
  | "InvalidRequest";

export class KiroError extends Error {
  readonly kind: KiroErrorKind;
  /** The auditor's stderr (verbatim), when the failure came from the subprocess. */
  readonly stderr?: string;
  /** The subprocess exit code, when applicable. */
  readonly exitCode?: number | null;

  constructor(
    kind: KiroErrorKind,
    message: string,
    opts?: { stderr?: string; exitCode?: number | null },
  ) {
    super(message);
    this.name = "KiroError";
    this.kind = kind;
    if (opts?.stderr !== undefined) this.stderr = opts.stderr;
    if (opts?.exitCode !== undefined) this.exitCode = opts.exitCode;
  }
}

/**
 * Map the auditor's failure (exit code + stderr) to a CLI-mirrored `KiroError`.
 * The Rust CLI prints `audit failed: <AdapterError Display>` on stderr; we match
 * the stable prefixes of each `AdapterError` variant's Display (crates/
 * fira-adapters/src/cli.rs) to recover its category. An unrecognized failure is
 * `AuditFailed` carrying the raw stderr (never silently swallowed).
 */
export function mapCliFailure(exitCode: number | null, stderr: string): KiroError {
  const text = stderr.trim();
  const opts = { stderr, exitCode };

  // The CLI wraps the AdapterError Display in `audit failed: …`; strip that
  // prefix if present so we match the inner Display.
  const inner = text.replace(/^audit failed:\s*/u, "");

  if (/^capability error:/u.test(inner)) {
    return new KiroError("Capability", inner, opts);
  }
  if (/^system classification is undetermined/u.test(inner)) {
    return new KiroError("UndeterminedNeedsProfile", inner, opts);
  }
  if (/^classifier proposes profile/u.test(inner)) {
    return new KiroError("NeedsExplicitConfirmation", inner, opts);
  }
  if (/^failed to persist report to audit workspace:/u.test(inner)) {
    return new KiroError("Persist", inner, opts);
  }
  // Output (OutputSink) and WorkspaceUnresolved Display strings are free-form;
  // disambiguate by their characteristic substrings where possible.
  if (/--workspace/u.test(inner) && /workspace/iu.test(inner)) {
    return new KiroError("WorkspaceUnresolved", inner, opts);
  }
  if (/outside the project|project tree|output/iu.test(inner)) {
    return new KiroError("Output", inner, opts);
  }
  return new KiroError("AuditFailed", inner.length > 0 ? inner : "audit failed", opts);
}
