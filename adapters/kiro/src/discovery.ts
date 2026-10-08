// Binary discovery + version pinning (requirements §9.5 / design §2.5).
//
// Resolution order: (1) an explicitly configured path; (2) `$PATH` (by invoking
// the bare command name `fira-guardian`, which the OS resolves via PATH). Before
// trusting ANY audit output, the resolved binary's `version` subcommand is run
// and its reported `contract-version` is asserted to equal the expected value.
//
// Every failure fails CLOSED: BinaryNotFound (cannot locate / cannot spawn),
// VersionMismatch (contract version differs), SpawnFailed (version call errored).
// No network discovery, download, installation, or implicit dependency install.

import { KiroError } from "./errors.ts";
import type { ProcessRunner } from "./invoke.ts";
import type { AuditorBinary } from "./types.ts";

/** The command the adapter runs. An explicit path overrides the PATH lookup. */
export function binaryCommand(bin: AuditorBinary): string {
  return bin.path !== undefined && bin.path.trim().length > 0 ? bin.path : "fira-guardian";
}

/** Parse the `contract-version: X` line from `fira-guardian version` stdout. */
export function parseContractVersion(stdout: string): string | undefined {
  for (const line of stdout.split(/\r?\n/u)) {
    const m = /^contract-version:\s*(.+?)\s*$/u.exec(line);
    if (m && m[1] !== undefined) return m[1];
  }
  return undefined;
}

/**
 * Resolve and trust the auditor binary: run `<cmd> version`, require a clean
 * exit and a `contract-version` matching `bin.expectedContractVersion`. Returns
 * the resolved command string on success; throws a fail-closed KiroError
 * otherwise. Must be called before any audit invocation.
 */
export function resolveAndVerify(bin: AuditorBinary, run: ProcessRunner): string {
  const cmd = binaryCommand(bin);
  const res = run(cmd, ["version"]);

  // Could not spawn / not found. spawnSync surfaces ENOENT as status null with
  // the error text in stderr; treat an absent status as a discovery failure.
  if (res.status === null) {
    const notFound = /ENOENT|not found|no such file/iu.test(res.stderr);
    throw new KiroError(
      notFound ? "BinaryNotFound" : "SpawnFailed",
      `could not run ${JSON.stringify(cmd)} version: ${res.stderr.trim() || "spawn failed"}`,
      { stderr: res.stderr, exitCode: res.status },
    );
  }
  if (res.status !== 0) {
    throw new KiroError(
      "SpawnFailed",
      `${JSON.stringify(cmd)} version exited with code ${res.status}`,
      { stderr: res.stderr, exitCode: res.status },
    );
  }

  const reported = parseContractVersion(res.stdout);
  if (reported === undefined) {
    throw new KiroError(
      "MalformedOutput",
      `${JSON.stringify(cmd)} version did not report a contract-version line`,
      { stderr: res.stderr, exitCode: res.status },
    );
  }
  if (reported !== bin.expectedContractVersion) {
    throw new KiroError(
      "VersionMismatch",
      `auditor contract-version ${JSON.stringify(reported)} != expected ${JSON.stringify(
        bin.expectedContractVersion,
      )} (fail closed)`,
      { stderr: res.stderr, exitCode: res.status },
    );
  }
  return cmd;
}
