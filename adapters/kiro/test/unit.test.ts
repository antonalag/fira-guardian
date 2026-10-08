// Unit tests for the Kiro adapter — pure logic with a FAKE process runner.
// No real binary, no filesystem writes, no network. Covers AC-2 (marshalling),
// AC-5a (version pin), AC-6 (errors), and the §9.2 read-only projection.

import { test } from "node:test";
import assert from "node:assert/strict";

import { proposeArgs, runArgs, assertProfileId } from "../src/marshal.ts";
import { binaryCommand, parseContractVersion, resolveAndVerify } from "../src/discovery.ts";
import { mapCliFailure, KiroError } from "../src/errors.ts";
import { runKiroAudit, proposeProfile } from "../src/index.ts";
import type { ProcessRunner, RunResult } from "../src/invoke.ts";
import type { AuditorBinary, KiroAuditRequest } from "../src/types.ts";

const EXPECTED = "1.0-frozen+corr1-4";
const bin: AuditorBinary = { path: "/opt/fira/fira-guardian", expectedContractVersion: EXPECTED };

/** A fake runner that answers `version` with a fixed contract line, and routes
 *  `audit` calls to a supplied handler. */
function fakeRunner(opts: {
  version?: string;
  versionStatus?: number | null;
  versionStderr?: string;
  audit?: (args: readonly string[]) => RunResult;
}): ProcessRunner {
  return (_cmd, args) => {
    if (args[0] === "version") {
      if (opts.versionStatus !== undefined && opts.versionStatus !== 0) {
        return { status: opts.versionStatus, stdout: "", stderr: opts.versionStderr ?? "" };
      }
      return {
        status: 0,
        stdout: `contract-version: ${opts.version ?? EXPECTED}\n`,
        stderr: "",
      };
    }
    if (args[0] === "audit" && opts.audit) return opts.audit(args);
    return { status: 1, stdout: "", stderr: "unexpected call" };
  };
}

const baseReq: KiroAuditRequest = { projectRoot: "/proj", releaseTarget: "v1" };

// --------------------------------------------------------------------------
// AC-2 — request → args marshalling
// --------------------------------------------------------------------------

test("AC-2: base args carry project, release-target, --format both; no execution_boundary flag", () => {
  const args = proposeArgs(baseReq);
  assert.deepEqual(args, [
    "audit",
    "--project",
    "/proj",
    "--release-target",
    "v1",
    "--format",
    "both",
  ]);
  assert.ok(!args.join(" ").includes("execution"), "execution_boundary is never passed");
  assert.ok(!args.includes("--output"), "no --output; canonical WS-1 artifacts are read");
});

test("AC-2: optional depth/workspace are included only when present", () => {
  const args = proposeArgs({ ...baseReq, requestedDepth: "deep", workspace: "/ws" });
  assert.ok(args.includes("--depth") && args.includes("deep"));
  assert.ok(args.includes("--workspace") && args.includes("/ws"));
});

test("AC-2/§9.4: confirmProposed → --yes; reclassify → --profile <kebab-id>", () => {
  assert.ok(runArgs(baseReq, { kind: "confirmProposed" }).includes("--yes"));
  // The CLI flag uses clap kebab-case; the wire ProfileId is snake_case.
  const r = runArgs(baseReq, { kind: "reclassify", profile: "web_service" });
  assert.ok(r.includes("--profile") && r.includes("web-service"), "wire snake_case → CLI kebab-case");
  assert.ok(!r.includes("web_service"), "the snake_case form is not passed to the CLI");
});

test("PROF-1: an unknown profile is rejected at marshalling", () => {
  assert.throws(() => assertProfileId("sixth_profile"), (e: unknown) => {
    return e instanceof KiroError && e.kind === "InvalidRequest";
  });
});

test("InvalidRequest: empty required fields fail closed before spawn", () => {
  assert.throws(() => proposeArgs({ projectRoot: "", releaseTarget: "v1" }));
  assert.throws(() => proposeArgs({ projectRoot: "/p", releaseTarget: "  " }));
});

// --------------------------------------------------------------------------
// AC-5a — binary discovery + version pin (fail closed)
// --------------------------------------------------------------------------

test("binaryCommand prefers the explicit path, else the bare command name", () => {
  assert.equal(binaryCommand(bin), "/opt/fira/fira-guardian");
  assert.equal(binaryCommand({ expectedContractVersion: EXPECTED }), "fira-guardian");
});

test("parseContractVersion extracts the version line", () => {
  assert.equal(parseContractVersion("a\ncontract-version: 1.0-frozen+corr1-4\nb"), EXPECTED);
  assert.equal(parseContractVersion("no version here"), undefined);
});

test("AC-5a: version mismatch fails closed with VersionMismatch", () => {
  const run = fakeRunner({ version: "9.9-other" });
  assert.throws(
    () => resolveAndVerify(bin, run),
    (e: unknown) => e instanceof KiroError && e.kind === "VersionMismatch",
  );
});

test("AC-5a: a missing binary fails closed with BinaryNotFound", () => {
  const run: ProcessRunner = () => ({ status: null, stdout: "", stderr: "spawn ENOENT" });
  assert.throws(
    () => resolveAndVerify(bin, run),
    (e: unknown) => e instanceof KiroError && e.kind === "BinaryNotFound",
  );
});

// --------------------------------------------------------------------------
// AC-6 — CLI failure mapping
// --------------------------------------------------------------------------

test("AC-6: mapCliFailure recovers each CLI AdapterError category", () => {
  assert.equal(mapCliFailure(1, "audit failed: capability error: boom").kind, "Capability");
  assert.equal(
    mapCliFailure(1, "audit failed: system classification is undetermined (empty)").kind,
    "UndeterminedNeedsProfile",
  );
  assert.equal(
    mapCliFailure(1, "audit failed: classifier proposes profile Library; re-run with --yes").kind,
    "NeedsExplicitConfirmation",
  );
  assert.equal(
    mapCliFailure(1, "audit failed: failed to persist report to audit workspace: disk").kind,
    "Persist",
  );
  assert.equal(mapCliFailure(1, "audit failed: something unrecognized").kind, "AuditFailed");
});

// --------------------------------------------------------------------------
// Two-phase confirm (AC-4) over the fake runner
// --------------------------------------------------------------------------

test("AC-4: proposeProfile surfaces a classifiable proposal", () => {
  const run = fakeRunner({
    audit: () => ({
      status: 1,
      stdout: "",
      stderr: "audit failed: classifier proposes profile Library; re-run with --yes",
    }),
  });
  const p = proposeProfile(bin, baseReq, { runner: run });
  assert.equal(p.undetermined, false);
  assert.equal(p.proposedProfile, "library");
});

test("AC-4: proposeProfile surfaces an undetermined classification", () => {
  const run = fakeRunner({
    audit: () => ({
      status: 1,
      stdout: "",
      stderr: "audit failed: system classification is undetermined (no manifests)",
    }),
  });
  const p = proposeProfile(bin, baseReq, { runner: run });
  assert.equal(p.undetermined, true);
});

test("AC-4: proposeProfile rethrows a non-gate failure", () => {
  const run = fakeRunner({
    audit: () => ({ status: 1, stdout: "", stderr: "audit failed: capability error: x" }),
  });
  assert.throws(
    () => proposeProfile(bin, baseReq, { runner: run }),
    (e: unknown) => e instanceof KiroError && e.kind === "Capability",
  );
});

// --------------------------------------------------------------------------
// AC-6: runKiroAudit maps a failing audit to a structured error
// --------------------------------------------------------------------------

test("AC-6: runKiroAudit maps a non-zero audit exit to a KiroError", () => {
  const run = fakeRunner({
    audit: () => ({
      status: 1,
      stdout: "",
      stderr: "audit failed: classifier proposes profile Library; re-run with --yes",
    }),
  });
  assert.throws(
    () => runKiroAudit(bin, baseReq, { kind: "confirmProposed" }, { runner: run }),
    (e: unknown) => e instanceof KiroError && e.kind === "NeedsExplicitConfirmation",
  );
});

test("AC-6: a malformed success (no canonical location line) fails closed", () => {
  const run = fakeRunner({
    audit: () => ({ status: 0, stdout: "{}", stderr: "ran, but no location line" }),
  });
  assert.throws(
    () => runKiroAudit(bin, baseReq, { kind: "confirmProposed" }, { runner: run }),
    (e: unknown) => e instanceof KiroError && e.kind === "MalformedOutput",
  );
});
