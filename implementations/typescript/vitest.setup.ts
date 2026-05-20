/**
 * Vitest global setup and teardown for SMT solver pool.
 *
 * Initializes the solver pool once at test suite startup and tears it
 * down cleanly after all tests complete. This avoids per-test spawn
 * overhead and prevents timeout flakes from process contention.
 *
 * The pool is only started when the z3 binary is reachable. Tests that
 * do not invoke the SMT solver run correctly without it; tests that call
 * withSolverWorker() will fail at acquire time if the pool was skipped.
 * This lets targeted runs (e.g. library-bindings lifter tests) proceed
 * on machines without z3 installed.
 */

import { spawnSync } from "child_process";
import { initGlobalPool, shutdownGlobalPool } from "./src/test-support/smtPool.js";

function z3Available(): boolean {
  try {
    const result = spawnSync("z3", ["--version"], { timeout: 3000 });
    return result.status === 0;
  } catch {
    return false;
  }
}

export async function setup(): Promise<void> {
  if (!z3Available()) {
    // z3 not found; SMT-backed tests will fail at acquire() if invoked.
    // Non-SMT tests (including library-bindings lifter tests) run normally.
    return;
  }
  const pool = initGlobalPool({ binary: "z3", poolSize: 4 });
  await pool.init();
}

export async function teardown(): Promise<void> {
  await shutdownGlobalPool();
  // Give process shutdown a moment to complete
  await new Promise((resolve) => setTimeout(resolve, 100));
}
