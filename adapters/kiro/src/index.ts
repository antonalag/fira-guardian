// FIRA Guardian — Kiro host shim (Task 13).
//
// A thin TypeScript adapter that drives the existing Rust `fira-guardian` CLI as
// a local subprocess over the TRANSPORT-1 CLI contract. It performs ONLY:
// transport, CLI argument marshalling, canonical report loading, read-only
// response projection, the two-phase confirmation UX, binary discovery/version
// validation, and error mapping. It contains NO audit logic and never
// recomputes, reinterprets, adjudicates, or modifies any verdict/report field.
// Rust FIRA remains authoritative; `report.json`/`report.md` are canonical.

import { existsSync, readFileSync } from "node:fs";
import { isAbsolute, join } from "node:path";

import { KiroError, mapCliFailure } from "./errors.ts";
import { resolveAndVerify } from "./discovery.ts";
import { defaultRunner, type ProcessRunner } from "./invoke.ts";
import { proposeArgs, runArgs } from "./marshal.ts";
import type {
  AuditorBinary,
  CanonicalReport,
  ConfirmDecision,
  ConfirmGateProposal,
  KiroAuditRequest,
  KiroAuditResponse,
  ProfileId,
} from "./types.ts";
import { PROFILE_IDS } from "./types.ts";

export * from "./types.ts";
export { KiroError } from "./errors.ts";
export type { KiroErrorKind } from "./errors.ts";
export type { ProcessRunner, RunResult } from "./invoke.ts";

/** Optional injection seam for tests (defaults to the real subprocess runner). */
export interface AdapterOptions {
  readonly runner?: ProcessRunner;
}

/** The stderr line the CLI prints with the canonical WS-1 location. */
const CANONICAL_LINE = /^canonical audit-workspace artifacts:\s*(.+?)\s*$/u;

function parseCanonicalLocation(stderr: string): string | undefined {
  for (const line of stderr.split(/\r?\n/u)) {
    const m = CANONICAL_LINE.exec(line);
    if (m && m[1] !== undefined) return m[1];
  }
  return undefined;
}

/**
 * Project the canonical `report.json` text into the read-only structured subset
 * the §18 AuditResponse exposes. This is a pure projection: it copies fields,
 * never recomputes or adjudicates. Unknown fields are ignored; required fields
 * missing ⇒ MalformedOutput (fail closed).
 */
function projectReport(machineReport: string): CanonicalReport {
  let parsed: unknown;
  try {
    parsed = JSON.parse(machineReport);
  } catch (e) {
    throw new KiroError("MalformedOutput", `canonical report.json is not valid JSON: ${String(e)}`);
  }
  if (typeof parsed !== "object" || parsed === null) {
    throw new KiroError("MalformedOutput", "canonical report.json is not a JSON object");
  }
  const obj = parsed as Record<string, unknown>;
  for (const key of [
    "schema_version",
    "audit_id",
    "technical_assessment",
    "human_decision",
    "findings",
    "gates",
    "coverage",
  ]) {
    if (!(key in obj)) {
      throw new KiroError("MalformedOutput", `canonical report.json missing field ${JSON.stringify(key)}`);
    }
  }
  // The shape is trusted to match the frozen S11 schema the Rust auditor
  // produced; we assert its top-level presence above and project read-only.
  return obj as unknown as CanonicalReport;
}

/** Read a required artifact file from the canonical location (fail closed). */
function readArtifact(dir: string, name: string): string {
  const path = join(dir, name);
  if (!existsSync(path)) {
    throw new KiroError(
      "MalformedOutput",
      `expected canonical artifact ${JSON.stringify(name)} not found at ${JSON.stringify(path)}`,
    );
  }
  try {
    return readFileSync(path, "utf8");
  } catch (e) {
    throw new KiroError("MalformedOutput", `could not read ${JSON.stringify(path)}: ${String(e)}`);
  }
}

/**
 * Phase 2 — run one audit end to end.
 *
 * 1. verify the binary + contract version (fail closed, §9.5);
 * 2. spawn `fira-guardian audit …` with the request marshalled to flags, plus
 *    the confirm decision mapped to `--yes`/`--profile` (§9.4);
 * 3. on non-zero exit, map the CLI failure to a structured KiroError (§2.4);
 * 4. on success, locate the canonical WS-1 artifacts, read `report.{json,md}`
 *    (the authoritative bytes), and project the structured §18 response
 *    read-only. No field is recomputed or edited (§9.2).
 */
export function runKiroAudit(
  bin: AuditorBinary,
  req: KiroAuditRequest,
  decision: ConfirmDecision,
  opts?: AdapterOptions,
): KiroAuditResponse {
  const run = opts?.runner ?? defaultRunner;
  const cmd = resolveAndVerify(bin, run);

  const res = run(cmd, runArgs(req, decision));
  if (res.status !== 0) {
    throw mapCliFailure(res.status, res.stderr);
  }

  const location = parseCanonicalLocation(res.stderr);
  if (location === undefined || !isAbsolute(location)) {
    throw new KiroError(
      "MalformedOutput",
      "could not determine the canonical audit-workspace location from the auditor output",
      { stderr: res.stderr, exitCode: res.status },
    );
  }

  const machineReport = readArtifact(location, "report.json");
  const humanReport = readArtifact(location, "report.md");
  const report = projectReport(machineReport);

  return {
    technicalAssessment: report.technical_assessment,
    humanDecision: report.human_decision,
    findings: report.findings,
    gates: report.gates,
    coverage: report.coverage,
    machineReport,
    humanReport,
    canonicalLocation: location,
  };
}

/**
 * Phase 1 — surface the P1 confirmation gate WITHOUT committing a verdict.
 *
 * Runs the CLI with no `--yes`/`--profile`. The non-interactive CLI then reports
 * its classify/confirm gate as a non-zero exit:
 *  - "classifier proposes profile X …"  ⇒ a confirmable proposal (profile X);
 *  - "system classification is undetermined …" ⇒ the human must pick a profile.
 * The adapter projects that into a `ConfirmGateProposal` for the host; it invents
 * no gate semantics (PROF-1) and emits no verdict. The host then calls
 * `runKiroAudit` with a `ConfirmDecision`.
 *
 * If the project is classifiable AND already non-interactively confirmable the
 * CLI would still require `--yes`; so phase 1 is expected to surface a gate, not
 * a finished audit. A clean exit here (no gate) is treated as MalformedOutput,
 * because a decision was expected.
 */
export function proposeProfile(
  bin: AuditorBinary,
  req: KiroAuditRequest,
  opts?: AdapterOptions,
): ConfirmGateProposal {
  const run = opts?.runner ?? defaultRunner;
  const cmd = resolveAndVerify(bin, run);

  const res = run(cmd, proposeArgs(req));
  if (res.status === 0) {
    // The CLI only succeeds non-interactively with --yes/--profile, which phase
    // 1 does not pass; a success here means the gate was bypassed unexpectedly.
    throw new KiroError(
      "MalformedOutput",
      "expected a confirmation gate but the auditor completed without one",
      { stderr: res.stderr, exitCode: res.status },
    );
  }

  const err = mapCliFailure(res.status, res.stderr);
  if (err.kind === "NeedsExplicitConfirmation") {
    const proposed = extractProposedProfile(err.message);
    return proposed !== undefined
      ? { proposedProfile: proposed, undetermined: false, message: err.message }
      : { undetermined: false, message: err.message };
  }
  if (err.kind === "UndeterminedNeedsProfile") {
    return { undetermined: true, message: err.message };
  }
  // Any other failure (capability, workspace, persist, …) is a real error.
  throw err;
}

/** Pull the proposed `ProfileId` out of the CLI's needs-confirmation message. */
function extractProposedProfile(message: string): ProfileId | undefined {
  // The Rust Display uses the debug form, e.g. "classifier proposes profile
  // Library;". Match case-insensitively against the known ids (both the wire
  // snake_case and the Rust CamelCase debug spelling).
  const camelToId: Record<string, ProfileId> = {
    library: "library",
    clitool: "cli_tool",
    webservice: "web_service",
    statefuldistributed: "stateful_distributed",
    batchpipeline: "batch_pipeline",
  };
  const m = /classifier proposes profile\s+([A-Za-z_]+)/u.exec(message);
  if (!m || m[1] === undefined) return undefined;
  const token = m[1].toLowerCase().replace(/_/gu, "");
  const id = camelToId[token];
  if (id !== undefined) return id;
  // Fallback: a direct wire-form match.
  const direct = m[1].toLowerCase();
  return (PROFILE_IDS as readonly string[]).includes(direct) ? (direct as ProfileId) : undefined;
}
