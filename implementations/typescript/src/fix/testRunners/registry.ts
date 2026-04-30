/**
 * Test runner registry — sixth primitive registry (alongside capability, relation,
 * intake, remediationLayer, artifactKind registries).
 *
 * Module-scoped Map. Same shape as the other five registries.
 */

export interface TestRunnerDescriptor {
  /** Runner name, e.g. "vitest", "jest", "mocha", "node-test", "none" */
  name: string;

  /** One-line description for help/audit. */
  description: string;

  /**
   * Detect whether this runner is in use for the given project.
   * `projectRoot` is the overlay's worktree path (not the user's repo).
   * Return a confidence score 0..1; 0 = not this runner.
   * Higher = more confident. Detection prefers the highest-scoring runner.
   */
  detect: (projectRoot: string) => number;

  /**
   * Resolve the runner binary's absolute path within the overlay's
   * node_modules. Throws if the binary doesn't resolve (caller may fall
   * through to a different runner or surface "no test runner").
   */
  resolveRunnerBinary: (projectRoot: string) => string;

  /**
   * Build the argv tail to invoke the runner against a single test file.
   * Receives a project-relative path (e.g., "src/foo.test.ts").
   * The full argv is `[resolveRunnerBinary, ...invocation(testFilePath)]`.
   */
  invocation: (testFilePath: string) => string[];

  /**
   * Parse the runner's outcome.
   * exitCode: 0 = passed; non-zero typically = failed but each runner has nuances.
   * Return passed/testCount/details for the audit trail.
   */
  parseOutcome: (exitCode: number, stdout: string, stderr: string) => {
    passed: boolean;
    testCount: number;
    details: string;
  };
}

const registry = new Map<string, TestRunnerDescriptor>();

/**
 * Register a test runner descriptor. Idempotent: duplicate names overwrite
 * (with a warning).
 */
export function registerTestRunner(d: TestRunnerDescriptor): void {
  if (registry.has(d.name)) {
    console.warn(
      `[testRunnerRegistry] duplicate registration for "${d.name}"; overwriting.`,
    );
  }
  registry.set(d.name, d);
}

/** Look up a descriptor by runner name. Returns undefined if not registered. */
export function getTestRunner(name: string): TestRunnerDescriptor | undefined {
  return registry.get(name);
}

/** All registered descriptors (read-only snapshot). */
export function listTestRunners(): readonly TestRunnerDescriptor[] {
  return Array.from(registry.values());
}

/** Clear the registry. ONLY for tests. */
export function _clearTestRunnerRegistry(): void {
  registry.clear();
}

/**
 * Detect the project's runner via highest-confidence descriptor.
 * Returns the descriptor or the "none" fallback.
 *
 * Walk-up workspace logic (for monorepos) is a follow-up; MVP checks
 * projectRoot only.
 */
export function detectTestRunner(projectRoot: string): TestRunnerDescriptor {
  const candidates = listTestRunners().map((d) => ({ d, score: d.detect(projectRoot) }));
  candidates.sort((a, b) => b.score - a.score);
  const winner = candidates[0];
  if (!winner || winner.score === 0) {
    return getTestRunner("none")!;
  }
  return winner.d;
}
