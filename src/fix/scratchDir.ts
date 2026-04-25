/**
 * Centralized scratch-directory creation for the fix loop.
 *
 * Every scratch dir (overlays, cherry-pick helpers, adversarial fixture
 * validators, structuredOutput agent dirs, fuzz scenarios, etc.) goes through
 * createScratchDir. Two reasons:
 *
 * 1. Test isolation. Tests that verify cleanup behavior — e.g., "no
 *    provekit-apply-* dirs should remain after applyBundle returns" — used
 *    to inspect the global tmpdir() and were poisoned by concurrent test
 *    runs creating their own scratch dirs in the same place. With the env
 *    override below, tests set PROVEKIT_SCRATCH_PARENT to a per-test
 *    directory and inspect ONLY that directory, regardless of what other
 *    tests are doing.
 *
 * 2. Production observability. A single env knob lets operators redirect
 *    all scratch creation to a managed disk (e.g., a tmpfs mount) without
 *    code changes. Useful when /tmp is small or shared with other workloads.
 *
 * Resolution order:
 *
 *   1. Explicit `parent` argument (highest priority — test/production
 *      callers that want a specific dir without involving env).
 *   2. PROVEKIT_SCRATCH_PARENT env var (test-suite-wide override).
 *   3. Per-stage env var: PROVEKIT_SCRATCH_PARENT_<UPPERCASE_PREFIX>
 *      where the prefix is normalized (trailing dash stripped, dashes →
 *      underscores). E.g., prefix "provekit-apply-" maps to
 *      PROVEKIT_SCRATCH_PARENT_PROVEKIT_APPLY.
 *   4. os.tmpdir() (default).
 *
 * Tests should prefer the env var so they don't have to thread an option
 * through every function in the call stack. Production code calls
 * createScratchDir(prefix) and gets the canonical behaviour.
 */

import { mkdtempSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

/**
 * Create a scratch directory under the resolved parent. The directory name
 * starts with `prefix` followed by random suffix (mkdtempSync semantics).
 *
 * `prefix` must end in a dash by convention (e.g. "provekit-apply-"). The
 * dash is the boundary between the prefix and the random suffix.
 */
export function createScratchDir(prefix: string, parent?: string): string {
  const resolvedParent =
    parent ??
    process.env.PROVEKIT_SCRATCH_PARENT ??
    process.env[perStageEnvVarName(prefix)] ??
    tmpdir();
  return mkdtempSync(join(resolvedParent, prefix));
}

/**
 * Normalize a prefix into the per-stage env var name.
 * "provekit-apply-"      → "PROVEKIT_SCRATCH_PARENT_PROVEKIT_APPLY"
 * "provekit-overlay-"    → "PROVEKIT_SCRATCH_PARENT_PROVEKIT_OVERLAY"
 * "regression-"          → "PROVEKIT_SCRATCH_PARENT_REGRESSION"
 */
function perStageEnvVarName(prefix: string): string {
  const normalized = prefix
    .replace(/-+$/, "")          // trailing dashes
    .replace(/-/g, "_")          // remaining dashes → underscores
    .toUpperCase();
  return `PROVEKIT_SCRATCH_PARENT_${normalized}`;
}
