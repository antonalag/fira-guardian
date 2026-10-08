// Request → CLI argument marshalling (requirements §2.1 / design §2.1).
//
// Pure functions: a KiroAuditRequest + confirm decision become an argument
// VECTOR for `fira-guardian audit` (no shell string — the caller passes this
// array straight to spawnSync, so a crafted release_target/path cannot inject
// shell syntax). `execution_boundary` is never emitted (CAP-1: the CLI fixes it
// to {READ, EXECUTE_EXISTING}; the adapter cannot widen it). Absent optionals
// are omitted, never defaulted to a surprising value.

import { KiroError } from "./errors.ts";
import {
  DEPTHS,
  PROFILE_IDS,
  type ConfirmDecision,
  type Depth,
  type KiroAuditRequest,
  type ProfileId,
} from "./types.ts";

/** Validate a value is one of the five profiles (PROF-1). */
export function assertProfileId(value: string): ProfileId {
  if ((PROFILE_IDS as readonly string[]).includes(value)) {
    return value as ProfileId;
  }
  throw new KiroError(
    "InvalidRequest",
    `unknown profile ${JSON.stringify(value)}; must be one of ${PROFILE_IDS.join(", ")} (PROF-1)`,
  );
}

/**
 * The CLI's `--profile` flag uses clap's kebab-case value names
 * (`cli-tool`, `web-service`, …), whereas the frozen wire/`ProfileId` form is
 * snake_case (`cli_tool`, …). This maps the wire `ProfileId` to the exact CLI
 * flag token. It is a transport detail of the existing CLI surface — not a new
 * profile and not a semantic change (PROF-1 unchanged; still exactly five).
 */
export function profileToCliFlag(id: ProfileId): string {
  return id.replace(/_/gu, "-");
}

/** Validate a value is a known depth. */
export function assertDepth(value: string): Depth {
  if ((DEPTHS as readonly string[]).includes(value)) {
    return value as Depth;
  }
  throw new KiroError(
    "InvalidRequest",
    `unknown depth ${JSON.stringify(value)}; must be one of ${DEPTHS.join(", ")}`,
  );
}

/** Reject obviously-malformed required inputs before spawning (fail closed). */
function validateRequest(req: KiroAuditRequest): void {
  if (req.projectRoot.trim().length === 0) {
    throw new KiroError("InvalidRequest", "projectRoot is required and must be non-empty");
  }
  if (req.releaseTarget.trim().length === 0) {
    throw new KiroError("InvalidRequest", "releaseTarget is required and must be non-empty");
  }
  if (req.auditProfile !== undefined) {
    assertProfileId(req.auditProfile);
  }
  if (req.requestedDepth !== undefined) {
    assertDepth(req.requestedDepth);
  }
}

/**
 * Build the base `audit` argument vector from a request (no confirm decision
 * applied). Shared by phase 1 (propose) and phase 2 (run).
 *
 * `--format both` is always set so both the machine and human reports are
 * available. `--profile`/`--yes` are added by the confirm-decision mapping, not
 * here.
 */
function baseAuditArgs(req: KiroAuditRequest): string[] {
  validateRequest(req);
  const args: string[] = [
    "audit",
    "--project",
    req.projectRoot,
    "--release-target",
    req.releaseTarget,
    "--format",
    "both",
  ];
  if (req.requestedDepth !== undefined) {
    args.push("--depth", req.requestedDepth);
  }
  if (req.workspace !== undefined) {
    args.push("--workspace", req.workspace);
  }
  // NOTE: no --output (the canonical WS-1 artifacts are what we read), and never
  // an execution-boundary flag (none exists; CAP-1 is fixed by the CLI).
  return args;
}

/**
 * Phase 1 (propose): run WITHOUT `--yes`/`--profile` so the CLI surfaces its P1
 * classify/confirm gate (either a NeedsExplicitConfirmation proposal or an
 * UndeterminedNeedsProfile error). The adapter reads that gate rather than
 * auto-confirming.
 *
 * Exception: if the request already carries an explicit `auditProfile`, the
 * human has effectively pre-reclassified; we still omit it in phase 1 so the
 * proposal reflects the *classifier's* view, and apply it only in phase 2.
 */
export function proposeArgs(req: KiroAuditRequest): string[] {
  return baseAuditArgs(req);
}

/**
 * Phase 2 (run): apply the human's confirm decision using the existing CLI
 * selection semantics (§9.4):
 *  - confirmProposed → `--yes` (confirm the classifier's proposed profile);
 *  - reclassify(id)  → `--profile <id>` (explicit human selection among the five).
 * The adapter never auto-confirms beyond these existing CLI rules.
 */
export function runArgs(req: KiroAuditRequest, decision: ConfirmDecision): string[] {
  const args = baseAuditArgs(req);
  switch (decision.kind) {
    case "confirmProposed":
      args.push("--yes");
      break;
    case "reclassify":
      args.push("--profile", profileToCliFlag(assertProfileId(decision.profile)));
      break;
  }
  return args;
}
