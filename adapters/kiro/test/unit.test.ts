// Unit tests for the Kiro adapter — pure logic with a FAKE process runner.
// No real binary and no network. The tests write no files, except the P-3
// projection test, which writes report.json/report.md into a temporary
// directory outside the repository and removes it in `finally`. Covers AC-2
// (marshalling), AC-5a (version pin), AC-6 (errors), and the §9.2 read-only
// projection.

import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";

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

// --------------------------------------------------------------------------
// P-3 (failure-classification-slice): success-path projection of INFRA-shaped
// fields through runKiroAudit, with a FAKE runner and a controlled report.json.
//
// WHAT THIS PROVES: the TypeScript adapter returns `gates[].cause`,
// `coverage.blocked_areas` and `coverage.known_limitations` unchanged, and that
// `machineReport`/`humanReport` are the exact bytes read from disk. Expected
// values are written as independent literals in the assertions (not derived by
// re-parsing the same file).
//
// WHAT THIS DOES NOT PROVE: that Rust produces these values or classifies INFRA
// correctly (see crates/fira-adapters/tests/failure_classification_e2e.rs,
// e2_persisted_report_contains_error_rows_and_blocked_areas, and
// crates/fira-core/tests/gate_eval_classification.rs). It does not validate the
// fixture against the S11 schema: the fixture carries only the seven top-level
// keys `projectReport` requires (src/index.ts), so it is NOT a complete S11
// report. The fixture is hand-written; it is not evidence of any real audit.
//
// Fixture references (Rust contract, read on commit 66be267):
//  - top-level keys: src/index.ts `projectReport`; crates/fira-core/src/report.rs
//    `AuditReport` (schema_version, audit_id, findings, gates, coverage,
//    technical_assessment, human_decision)
//  - Gate fields: crates/fira-core/src/gate.rs `Gate` (name, requirement_level,
//    state, rationale, cause?, support_mappings, supporting_findings)
//  - GateCause `APP|INFRA|NONE`: crates/fira-core/src/model/enums.rs
//  - SupportMapping fields: crates/fira-core/src/evidence.rs `SupportMapping`;
//    shape of the INFRA mapping: crates/fira-core/src/gate_eval/mod.rs
//    `support_mapping_for` (InconclusiveInfra)
//  - CoverageStatement / BlockedArea / ExecutedMechanism:
//    crates/fira-core/src/coverage.rs (all seven coverage arrays present)
//  - blocked area naming (`area` = gate name, `execution_result_ref` = command
//    id): crates/fira-adapters/src/cli.rs `infra_blocked_areas`
//  - ERROR/INFRA note text copied from crates/fira-adapters/src/cli.rs
//    `error_note` (InfraLevel branch)
//  - assessment fields and `computed_by_rule`: crates/fira-core/src/assessment.rs
//    `TechnicalAssessment`; crates/fira-adapters/src/cli.rs `run_audit`
//  - valid-value shapes: schemas/examples/valid/s11-audit-report.json
// --------------------------------------------------------------------------

test("P-3: runKiroAudit projects INFRA cause, blocked_areas and notes unchanged (fake runner, controlled report)", () => {
  const infraNote = (id: string): string =>
    `Declared verification mechanism '${id}' could not be started (outcome ERROR, INFRA_LEVEL): ` +
    "the operating system reported that a launcher selected by FIRA could not be found or " +
    "executed (attribution based on the operating system error kind only; the exact cause is " +
    "not established); the mechanism was not executed and provides no evidence.";
  const infraGate = (name: string, id: string) => ({
    name,
    requirement_level: "required",
    state: "UNKNOWN",
    rationale: `fixture rationale for ${name}`,
    cause: "INFRA",
    support_mappings: [
      {
        claim: `declared mechanism ${id} executed with outcome Error`,
        evidence_refs: [id],
        relevant_span: "Makefile:1",
        sufficiency: "INSUFFICIENT",
        unverified_remainder:
          "mechanism could not be evaluated because of an infrastructure impediment " +
          "(Error, INFRA_LEVEL); it provides no evidence about the application",
      },
    ],
    supporting_findings: [],
  });
  const report = {
    schema_version: "1.0.0",
    audit_id: "audit-fixture-p3",
    findings: [],
    gates: [infraGate("Build", "make-build"), infraGate("Tests", "make-test")],
    coverage: {
      audited_areas: [],
      skipped_areas: [],
      blocked_areas: [
        { area: "Build", blocking_condition: "fixture blocking condition (build)", execution_result_ref: "make-build" },
        { area: "Tests", blocking_condition: "fixture blocking condition (tests)", execution_result_ref: "make-test" },
      ],
      executed_mechanisms: [
        { command_id: "make-build", outcome: "ERROR" },
        { command_id: "make-test", outcome: "ERROR" },
      ],
      known_limitations: [infraNote("make-build"), infraNote("make-test")],
      assumptions: [],
      unanchored_hypotheses: [],
    },
    technical_assessment: {
      result: "NOT_READY",
      blocking_findings: [],
      blocking_gates: ["Build", "Tests"],
      risk_findings: [],
      risk_gates: [],
      computed_by_rule: "verdict-engine/§11",
    },
    human_decision: { state: "PENDING" },
  };
  const reportText = JSON.stringify(report, null, 2) + "\n";
  const markdownText = "# fixture report.md (not produced by FIRA)\n";

  // The directory must be outside the repository (never write into the project).
  const repoRoot = resolve(import.meta.dirname, "..", "..", "..");
  const dir = mkdtempSync(join(tmpdir(), "fira-p3-proj-"));
  try {
    assert.ok(
      !(dir + sep).startsWith(repoRoot + sep),
      "temp dir must be outside the repository",
    );
    writeFileSync(join(dir, "report.json"), reportText);
    writeFileSync(join(dir, "report.md"), markdownText);

    const run = fakeRunner({
      audit: () => ({
        status: 0,
        stdout: "",
        stderr: `canonical audit-workspace artifacts: ${dir}\n`,
      }),
    });
    const res = runKiroAudit(bin, baseReq, { kind: "confirmProposed" }, { runner: run });

    // Canonical bytes are returned exactly as written.
    assert.equal(res.canonicalLocation, dir);
    assert.equal(res.machineReport, reportText);
    assert.equal(res.humanReport, markdownText);

    // gates[].cause (not modeled in the TS `Gate` type, so read through a cast).
    const causes = res.gates.map((g) => [g.name, g.state, (g as unknown as { cause?: string }).cause]);
    assert.deepEqual(causes, [
      ["Build", "UNKNOWN", "INFRA"],
      ["Tests", "UNKNOWN", "INFRA"],
    ]);

    // coverage.blocked_areas: area, blocking_condition, execution_result_ref.
    assert.deepEqual(res.coverage.blocked_areas, [
      {
        area: "Build",
        blocking_condition: "fixture blocking condition (build)",
        execution_result_ref: "make-build",
      },
      {
        area: "Tests",
        blocking_condition: "fixture blocking condition (tests)",
        execution_result_ref: "make-test",
      },
    ]);

    // coverage.known_limitations: both INFRA notes, complete and in order.
    // Expected values are independent literals (not built with `infraNote`).
    assert.equal(res.coverage.known_limitations.length, 2);
    assert.deepEqual(res.coverage.known_limitations, [
      "Declared verification mechanism 'make-build' could not be started (outcome ERROR, INFRA_LEVEL): the operating system reported that a launcher selected by FIRA could not be found or executed (attribution based on the operating system error kind only; the exact cause is not established); the mechanism was not executed and provides no evidence.",
      "Declared verification mechanism 'make-test' could not be started (outcome ERROR, INFRA_LEVEL): the operating system reported that a launcher selected by FIRA could not be found or executed (attribution based on the operating system error kind only; the exact cause is not established); the mechanism was not executed and provides no evidence.",
    ]);

    // Other projected sub-objects keep their literal values.
    assert.deepEqual(res.coverage.executed_mechanisms, [
      { command_id: "make-build", outcome: "ERROR" },
      { command_id: "make-test", outcome: "ERROR" },
    ]);
    assert.equal(res.technicalAssessment.result, "NOT_READY");
    assert.deepEqual(res.technicalAssessment.blocking_gates, ["Build", "Tests"]);
    assert.equal(res.humanDecision.state, "PENDING");
    assert.deepEqual(res.findings, []);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  assert.ok(!existsSync(dir), "temp dir is removed");
});
