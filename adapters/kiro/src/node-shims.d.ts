// Minimal ambient declarations for the subset of the Node.js API this package
// uses. Declared locally so the package type-checks fully OFFLINE, with no
// `@types/node` dependency (no network install — Task 13 constraint). This is a
// *typing* shim only; it adds no runtime code and reimplements nothing.

declare module "node:child_process" {
  export interface SpawnSyncResult {
    status: number | null;
    signal: string | null;
    stdout: string;
    stderr: string;
    error?: Error;
  }
  export interface SpawnSyncOptions {
    cwd?: string;
    encoding?: "utf8";
    timeout?: number;
    maxBuffer?: number;
    // Explicitly NOT inheriting a shell: args are passed as an array (no shell
    // interpolation). `shell` is intentionally omitted from our call sites.
    env?: Record<string, string | undefined>;
  }
  export function spawnSync(
    command: string,
    args: readonly string[],
    options?: SpawnSyncOptions,
  ): SpawnSyncResult;
}

declare module "node:fs" {
  export function readFileSync(path: string, encoding: "utf8"): string;
  export function existsSync(path: string): boolean;
  export function writeFileSync(path: string, data: string): void;
  export function mkdtempSync(prefix: string): string;
  export function rmSync(path: string, options?: { recursive?: boolean; force?: boolean }): void;
}

declare module "node:os" {
  export function tmpdir(): string;
}

declare module "node:path" {
  export function join(...parts: string[]): string;
  export function resolve(...parts: string[]): string;
  export function isAbsolute(p: string): boolean;
  export const sep: string;
}

declare module "node:test" {
  type TestFn = () => void | Promise<void>;
  export function test(name: string, fn: TestFn): void;
  export function describe(name: string, fn: () => void): void;
  export function it(name: string, fn: TestFn): void;
}

declare module "node:assert/strict" {
  interface AssertStrict {
    (value: unknown, message?: string): asserts value;
    equal(actual: unknown, expected: unknown, message?: string): void;
    deepEqual(actual: unknown, expected: unknown, message?: string): void;
    notEqual(actual: unknown, expected: unknown, message?: string): void;
    ok(value: unknown, message?: string): asserts value;
    throws(
      fn: () => unknown,
      expected?: string | RegExp | ((err: unknown) => boolean),
      message?: string,
    ): void;
    match(value: string, regexp: RegExp, message?: string): void;
  }
  const assert: AssertStrict;
  export default assert;
}

declare const process: {
  env: Record<string, string | undefined>;
  platform: string;
};

interface ImportMeta {
  /** Absolute path of the directory containing the current module (Node >=20.11). */
  readonly dirname: string;
  readonly filename: string;
  readonly url: string;
}
