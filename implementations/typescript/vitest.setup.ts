/**
 * Vitest global setup and teardown for SMT solver pool.
 *
 * Initializes the solver pool once at test suite startup and tears it
 * down cleanly after all tests complete. This avoids per-test spawn
 * overhead and prevents timeout flakes from process contention.
 */

import { initGlobalPool, shutdownGlobalPool } from "./src/test-support/smtPool.js";

export async function setup(): Promise<void> {
  const pool = initGlobalPool({ binary: "z3", poolSize: 4 });
  await pool.init();
}

export async function teardown(): Promise<void> {
  await shutdownGlobalPool();
}
