// End-to-end tests: drive the REAL `fira-guardian` binary as a subprocess
// against temp fixture projects. No network; filesystem only (temp dirs for the
// project and the WS-1 workspace). Mirrors the behavioral cases in the Rust
// `crates/fira-adapters/tests/cli.rs`, exercised through the TypeScript adapter.
//
// If the binary is not built, these tests fail closed (they do not silently
// skip), matching the adapter's own discovery policy.

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";

import { runKiroAudit, proposeProfile, KiroError } from "../src/index.ts";
import type { ProcessRunner } from "../src/index.ts";
import type { AuditorBinary, KiroAuditRequest } from "../src/types.ts";

const EXPECTED = "1.0-frozen+corr1-4";

/** The built debug binary, four levels up from this test file's package. */
const BINARY = join(import.meta.dirname, "..", "..", "..", "target", "debug", "fira-guardian");
const bin: AuditorBinary = { path: BINARY, expectedContractVersion: EXPECTED };

function requireBinary(): void {
  if (!existsSync(BINARY)) {
    throw new Error(
      `fira-guardian not built at ${BINARY}; run \`cargo build -p fira-cli\` first (tests fail closed, never skip)`,
    );
  }
}

function tmp(prefix: string): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

/** A library-shaped Cargo project ⇒ classifier proposes `library`. */
function libraryProject(): string {
  const dir = tmp("fira-proj-lib-");
  writeFileSync(
    join(dir, "Cargo.toml"),
    '[package]\nname = "demo"\nversion = "0.1.0"\n\n[lib]\nname = "demo"\n',
  );
  return dir;
}

/** An empty project ⇒ classifier Undetermined. */
function emptyProject(): string {
  return tmp("fira-proj-empty-");
}

function cleanup(...dirs: string[]): void {
  for (const d of dirs) rmSync(d, { recursive: true, force: true });
}

// --------------------------------------------------------------------------
// AC-1 / AC-3 / AC-7 — subprocess invocation, response projection, determinism
// --------------------------------------------------------------------------

test("AC-1/AC-3: runKiroAudit (confirmProposed) returns a response whose fields match the canonical report.{json,md}", () => {
  requireBinary();
  const project = libraryProject();
  const workspace = tmp("fira-ws-");
  try {
    const req: KiroAuditRequest = { projectRoot: project, releaseTarget: "v1", workspace };
    const res = runKiroAudit(bin, req, { kind: "confirmProposed" }, {});

    // Canonical artifacts exist at the reported location.
    const jsonPath = join(res.canonicalLocation, "report.json");
    const mdPath = join(res.canonicalLocation, "report.md");
    assert.ok(existsSync(jsonPath), "report.json persisted (WS-1)");
    assert.ok(existsSync(mdPath), "report.md persisted (WS-1)");

    // machineReport / humanReport are the canonical bytes verbatim (§9.2).
    assert.equal(res.machineReport, readFileSync(jsonPath, "utf8"));
    assert.equal(res.humanReport, readFileSync(mdPath, "utf8"));

    // The structured projection equals the canonical JSON (read-only, no edits).
    const canonical = JSON.parse(res.machineReport) as Record<string, unknown>;
    assert.deepEqual(res.technicalAssessment, canonical["technical_assessment"]);
    assert.deepEqual(res.humanDecision, canonical["human_decision"]);
    assert.deepEqual(res.findings, canonical["findings"]);
    assert.deepEqual(res.gates, canonical["gates"]);
    assert.deepEqual(res.coverage, canonical["coverage"]);
  } finally {
    cleanup(project, workspace);
  }
});

test("AC-7: two runs on the same fixture yield the same forwarded verdict", () => {
  requireBinary();
  const project = libraryProject();
  const ws1 = tmp("fira-ws-");
  const ws2 = tmp("fira-ws-");
  try {
    const a = runKiroAudit(
      bin,
      { projectRoot: project, releaseTarget: "v1", workspace: ws1 },
      { kind: "confirmProposed" },
    );
    const b = runKiroAudit(
      bin,
      { projectRoot: project, releaseTarget: "v1", workspace: ws2 },
      { kind: "confirmProposed" },
    );
    assert.deepEqual(a.technicalAssessment, b.technicalAssessment);
    assert.deepEqual(a.gates, b.gates);
  } finally {
    cleanup(project, ws1, ws2);
  }
});

// --------------------------------------------------------------------------
// AC-4 — two-phase confirmation gate against the real CLI
// --------------------------------------------------------------------------

test("AC-4: proposeProfile surfaces the library proposal, then confirm runs it", () => {
  requireBinary();
  const project = libraryProject();
  const workspace = tmp("fira-ws-");
  try {
    const req: KiroAuditRequest = { projectRoot: project, releaseTarget: "v1", workspace };

    const proposal = proposeProfile(bin, req, {});
    assert.equal(proposal.undetermined, false);
    assert.equal(proposal.proposedProfile, "library");

    const res = runKiroAudit(bin, req, { kind: "confirmProposed" }, {});
    const canonical = JSON.parse(res.machineReport) as { applied_profile: { profile_id: string } };
    assert.equal(canonical.applied_profile.profile_id, "library");
  } finally {
    cleanup(project, workspace);
  }
});

test("AC-4: reclassify maps to --profile and changes the applied profile", () => {
  requireBinary();
  const project = libraryProject();
  const workspace = tmp("fira-ws-");
  try {
    const res = runKiroAudit(
      bin,
      { projectRoot: project, releaseTarget: "v1", workspace },
      { kind: "reclassify", profile: "cli_tool" },
    );
    const canonical = JSON.parse(res.machineReport) as { applied_profile: { profile_id: string } };
    assert.equal(canonical.applied_profile.profile_id, "cli_tool");
  } finally {
    cleanup(project, workspace);
  }
});

test("AC-4/AC-6: an undetermined project surfaces UndeterminedNeedsProfile", () => {
  requireBinary();
  const project = emptyProject();
  const workspace = tmp("fira-ws-");
  try {
    const proposal = proposeProfile(bin, { projectRoot: project, releaseTarget: "v1", workspace }, {});
    assert.equal(proposal.undetermined, true);
  } finally {
    cleanup(project, workspace);
  }
});

// --------------------------------------------------------------------------
// AC-5 — capability / containment boundary (enforced by the Rust runtime)
// --------------------------------------------------------------------------

test("AC-5: a workspace inside the project tree is rejected (WS-1), surfaced as a structured error", () => {
  requireBinary();
  const project = libraryProject();
  try {
    // Point the workspace INSIDE the project tree — the runtime WorkspaceSink
    // must refuse this; the adapter surfaces it as a structured KiroError.
    const insideWs = join(project, "nested-ws");
    assert.throws(
      () =>
        runKiroAudit(
          bin,
          { projectRoot: project, releaseTarget: "v1", workspace: insideWs },
          { kind: "confirmProposed" },
        ),
      (e: unknown) => e instanceof KiroError,
    );
    // And nothing was written at the rejected in-project workspace path.
    assert.ok(!existsSync(join(insideWs, "report.json")));
  } finally {
    cleanup(project);
  }
});

// --------------------------------------------------------------------------
// AC-5a — version pinning against the real binary
// --------------------------------------------------------------------------

test("AC-5a: a wrong expected contract version fails closed (VersionMismatch)", () => {
  requireBinary();
  const project = libraryProject();
  const workspace = tmp("fira-ws-");
  try {
    const wrongBin: AuditorBinary = { path: BINARY, expectedContractVersion: "0.0-not-real" };
    assert.throws(
      () =>
        runKiroAudit(
          wrongBin,
          { projectRoot: project, releaseTarget: "v1", workspace },
          { kind: "confirmProposed" },
        ),
      (e: unknown) => e instanceof KiroError && e.kind === "VersionMismatch",
    );
  } finally {
    cleanup(project, workspace);
  }
});

// --------------------------------------------------------------------------
// P-3 — real INFRA failure: declared launcher cannot be found
// --------------------------------------------------------------------------

/**
 * A `ProcessRunner` identical in contract to the adapter's default runner
 * (argument vector, no shell, UTF-8, spawn errors surfaced as `status: null`),
 * except that the child's environment is the current one with ONLY `PATH`
 * replaced. `process.env` itself is never modified. The binary is invoked by
 * its absolute path, so its own resolution does not depend on `PATH`.
 */
function runnerWithPath(pathValue: string): ProcessRunner {
  return (command, args, options) => {
    const result = spawnSync(command, args, {
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      env: { ...process.env, PATH: pathValue },
      ...(options?.cwd !== undefined ? { cwd: options.cwd } : {}),
    });
    if (result.error) {
      return { status: null, stdout: result.stdout ?? "", stderr: String(result.error) };
    }
    return { status: result.status, stdout: result.stdout ?? "", stderr: result.stderr ?? "" };
  };
}

test("P-3: a missing launcher yields ERROR/INFRA rows, blocked_areas and notes in the persisted report, preserved by the adapter", () => {
  requireBinary();
  const project = tmp("fira-proj-make-");
  const workspace = tmp("fira-ws-");
  const emptyPathDir = tmp("fira-empty-path-");
  try {
    // Discovery is file-based: the Makefile declares the mechanisms, `make` is
    // never needed (and, with the empty PATH below, cannot be found).
    writeFileSync(join(project, "Makefile"), "build:\n\ttrue\ntest:\n\ttrue\nlint:\n\ttrue\n");

    const res = runKiroAudit(
      bin,
      { projectRoot: project, releaseTarget: "v1", workspace },
      { kind: "reclassify", profile: "cli_tool" },
      { runner: runnerWithPath(emptyPathDir) },
    );

    // Read the persisted report independently of the adapter's response.
    const persisted = JSON.parse(
      readFileSync(join(res.canonicalLocation, "report.json"), "utf8"),
    ) as {
      technical_assessment: { result: string };
      gates: { name: string; state: string; cause?: string }[];
      coverage: {
        executed_mechanisms: { command_id: string; outcome: string }[];
        blocked_areas: { area: string; blocking_condition: string; execution_result_ref: string }[];
        known_limitations: string[];
      };
    };

    // Rows: every declared mechanism failed to start.
    const ids = ["make-build", "make-test", "make-lint"];
    for (const id of ids) {
      const row = persisted.coverage.executed_mechanisms.find((r) => r.command_id === id);
      assert.equal(row?.outcome, "ERROR", `${id} outcome`);
    }

    // Gates backed by those mechanisms: UNKNOWN with an INFRA cause.
    for (const name of ["Build", "Tests"]) {
      const g = persisted.gates.find((x) => x.name === name);
      assert.equal(g?.state, "UNKNOWN", `${name} state`);
      assert.equal(g?.cause, "INFRA", `${name} cause`);
    }
    assert.ok(
      persisted.gates
        .filter((g) => g.name !== "Build" && g.name !== "Tests")
        .every((g) => g.cause !== "INFRA"),
      "only Build and Tests have an INFRA cause",
    );

    // blocked_areas: exactly Build -> make-build and Tests -> make-test.
    const blocked = persisted.coverage.blocked_areas;
    assert.deepEqual(
      blocked.map((b) => [b.area, b.execution_result_ref]).sort(),
      [
        ["Build", "make-build"],
        ["Tests", "make-test"],
      ],
    );
    for (const b of blocked) {
      assert.ok(
        persisted.coverage.executed_mechanisms.some((r) => r.command_id === b.execution_result_ref),
        `dangling execution_result_ref ${b.execution_result_ref}`,
      );
      assert.ok(b.blocking_condition.length > 0, `blocking_condition for ${b.area}`);
    }
    assert.ok(!blocked.some((b) => b.execution_result_ref === "make-lint"), "make-lint is not blocked");

    // Notes: one per ERROR mechanism, INFRA-classified and hedged.
    for (const id of ids) {
      const notes = persisted.coverage.known_limitations.filter((l) => l.includes(`'${id}'`));
      assert.equal(notes.length, 1, `exactly one note for ${id}`);
      assert.ok(notes[0]!.includes("could not be started (outcome ERROR, INFRA_LEVEL)"), notes[0]);
      assert.ok(notes[0]!.includes("exact cause is not established"), notes[0]);
      assert.ok(!notes[0]!.includes("APP_LEVEL"), notes[0]);
    }

    assert.equal(persisted.technical_assessment.result, "NOT_READY");

    // The adapter's projection preserves the persisted data.
    assert.deepEqual(res.gates, persisted.gates);
    assert.deepEqual(res.coverage, persisted.coverage);
    const projectedCauses = res.gates
      .filter((g) => g.name === "Build" || g.name === "Tests")
      .map((g) => (g as unknown as { cause?: string }).cause);
    assert.deepEqual(projectedCauses, ["INFRA", "INFRA"]);
    assert.equal(res.technicalAssessment.result, "NOT_READY");
  } finally {
    cleanup(project, workspace, emptyPathDir);
  }
});
