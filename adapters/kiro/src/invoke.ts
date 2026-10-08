// Thin, injectable subprocess runner (TRANSPORT-1: a local child process only).
//
// A single indirection over `spawnSync` so the whole adapter can be unit-tested
// with a fake runner (no real binary, no network, no shell). Production wiring
// uses the real `node:child_process` spawnSync with an explicit argument array
// (no `shell`), so request values can never be interpreted as shell syntax.

import { spawnSync } from "node:child_process";

/** The captured result of one subprocess invocation. */
export interface RunResult {
  readonly status: number | null;
  readonly stdout: string;
  readonly stderr: string;
}

/**
 * Runs `command args…` and captures stdout/stderr/exit. Implementations MUST
 * pass `args` as a vector (never a shell string) and MUST NOT open any network.
 */
export type ProcessRunner = (
  command: string,
  args: readonly string[],
  options?: { readonly cwd?: string },
) => RunResult;

/** The default runner: `node:child_process` spawnSync, no shell, UTF-8. */
export const defaultRunner: ProcessRunner = (command, args, options) => {
  const base = { encoding: "utf8" as const, maxBuffer: 64 * 1024 * 1024 };
  const result = spawnSync(
    command,
    args,
    options?.cwd !== undefined ? { ...base, cwd: options.cwd } : base,
  );
  if (result.error) {
    // Surface spawn failures (e.g. ENOENT) to the caller via a sentinel status;
    // discovery/invocation layers translate this into a KiroError.
    return { status: null, stdout: result.stdout ?? "", stderr: String(result.error) };
  }
  return {
    status: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
};
