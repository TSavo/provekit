/**
 * Shared interface for test-framework adapters.
 *
 * Each adapter knows how to invoke one test runner (vitest, jest, mocha,
 * etc.) against a specific test file / test name and return a structured
 * outcome. Adapters are small, independent, and framework-specific —
 * they do not share state and they do not parse each other's output.
 *
 * The pipeline uses adapters via lookup in the registry (./index.ts).
 * Adding a new framework is a single new file implementing TestAdapter,
 * plus one registration line in the registry.
 */

export type TestOutcomeKind =
  | "pass"           // test ran and its assertions held
  | "fail"           // test ran and an assertion failed
  | "error"          // test threw an uncaught error (not an assertion failure)
  | "skipped"        // test was skipped by the framework (e.g., .skip, .only exclusion)
  | "timeout"        // adapter killed the process after exceeding timeoutMs
  | "adapter-error"; // adapter itself failed (couldn't spawn, couldn't parse output, etc.)

export interface TestInvocation {
  /** Absolute or project-relative path to the test file. */
  testFile: string;

  /**
   * Optional test-name filter. When provided, the adapter runs only the
   * test whose block name matches this string. Matching semantics are
   * framework-specific (vitest uses -t substring, jest uses -t regex,
   * mocha uses --grep). When omitted, the adapter runs every test in
   * the file.
   */
  testName?: string;

  /** Project root — used to resolve relative paths and spawn the runner. */
  projectRoot: string;

  /** Hard deadline for the entire invocation. Adapters must kill the child process on timeout. */
  timeoutMs: number;
}

export interface TestOutcome {
  kind: TestOutcomeKind;

  /**
   * One-sentence or short-paragraph description of the outcome suitable
   * for logging. On pass, usually "ok — <duration>ms." On fail/error,
   * includes the framework's failure message (assertion text, stack top).
   * On adapter-error, describes what went wrong inside the adapter
   * (spawn failure, unparseable output).
   */
  message: string;

  /** Wall-clock milliseconds from adapter invocation to outcome determination. */
  durationMs: number;

  /**
   * Raw stdout captured from the test runner. Truncated to some
   * reasonable size (typically 8KB) to keep reports manageable.
   * Optional: adapters that can't capture stdout (rare) may omit this.
   */
  rawOutput?: string;
}

export interface TestAdapter {
  /** Canonical framework identifier — must match the value returned by detectTestFramework. */
  readonly framework: string;

  /** Human-readable adapter name, e.g. "vitest JSON reporter". */
  readonly name: string;

  /** Execute one invocation. Never throws — errors are returned as TestOutcome with kind "adapter-error". */
  runTest(invocation: TestInvocation): Promise<TestOutcome>;
}
