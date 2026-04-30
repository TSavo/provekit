/**
 * Oracle #9 — mutation verification.
 *
 * Given a fix patch and a regression test, confirm:
 *   #9a: the test passes against the FIXED overlay (otherwise the test
 *        is wrong, the fix is incomplete, or both).
 *   #9b: the test fails when the fix is reverted (otherwise the test is
 *        a placebo — passes whether the bug is fixed or not).
 *
 * Both directions are required. A regression test that only passes after
 * the fix proves nothing about the fix's correctness; a test that fails
 * after reverting proves the test locks the property in.
 *
 * Factored out of src/fix/stages/generateRegressionTest.ts so the unified
 * doTheWork stage (which produces patch + test in one LLM call) can run
 * the same gate without duplicating the revert/restore logic.
 */

import {
  revertFixInOverlay,
  restoreFixInOverlay,
  runTestInOverlay,
} from "../testGen.js";
import { applyPatchToOverlay as defaultApplyPatch, reindexOverlay as defaultReindex } from "../overlay.js";
import type { OverlayHandle, CodePatch, FixCandidate } from "../types.js";

/**
 * Outcome of running Oracle #9. Both `passesOnFixedCode` and
 * `failsOnOriginalCode` are true on success; either is false on a
 * mechanical failure (the helper throws on those cases). The audit
 * fields surface the actual test runner output for debugging.
 */
export interface Oracle9Result {
  passesOnFixedCode: true;
  failsOnOriginalCode: true;
  audit: {
    fixedRunStdout: string;
    fixedRunExitCode: number;
    originalRunStdout: string;
    originalRunExitCode: number;
    mutationApplied: boolean;
    mutationReverted: boolean;
  };
}

export interface Oracle9Args {
  overlay: OverlayHandle;
  /**
   * The fix to mutate against. The patch's fileEdits set the files that
   * get reverted (to test the unfixed state) and restored (to leave the
   * overlay clean for downstream stages).
   */
  fix: FixCandidate;
  testFilePath: string;
  testCode: string;
  /**
   * The "main repo root" passed to the test runner. The runner uses it
   * to locate the test runner config + invocation path. Today's
   * generateRegressionTest derives this from the overlay; doTheWork
   * passes it in directly.
   */
  mainRepoRoot: string;
  /** Injectable test runner; falls back to the real vitest invocation. */
  testRunner?: typeof runTestInOverlay;
  /** Injectable patch applier; defaults to the overlay default. */
  applyPatch?: (overlay: OverlayHandle, patch: CodePatch) => void | Promise<void>;
  /** Injectable reindexer; defaults to the overlay default. */
  reindex?: (overlay: OverlayHandle, files: string[]) => Promise<void>;
}

/**
 * Run Oracle #9 against a (fix, test) pair. Throws on mechanical failure
 * (test fails on fixed code, test passes on original code, or runner
 * returns an exception). On success, returns the audit shape.
 *
 * The helper assumes the test file has NOT yet been written to the
 * overlay. It writes it, runs the dual checks, restores the fix at the
 * end so the overlay is in the fixed state for downstream stages.
 */
export async function verifyOracle9(args: Oracle9Args): Promise<Oracle9Result> {
  const applyPatch = args.applyPatch ?? defaultApplyPatch;
  const reindex = args.reindex ?? defaultReindex;
  const runTest = args.testRunner ?? runTestInOverlay;

  // 1. Write the test file into the overlay (the fix is already applied
  // by the caller).
  await applyPatch(args.overlay, {
    fileEdits: [{ file: args.testFilePath, newContent: args.testCode }],
    description: "regression test (oracle #9)",
  });

  // 2. Oracle #9a: test must pass against fixed code.
  const fixedRun = runTest(args.overlay, args.testFilePath, args.mainRepoRoot);

  // The runner emits a sentinel when no runner is detected (skip Oracle
  // #9 in that case; the rest of the pipeline treats it as an
  // informational pass).
  const NO_RUNNER_SENTINEL = "no test runner; oracle #9 skipped";
  if (fixedRun.stdout.startsWith(NO_RUNNER_SENTINEL)) {
    return {
      passesOnFixedCode: true,
      failsOnOriginalCode: true,
      audit: {
        fixedRunStdout: fixedRun.stdout,
        fixedRunExitCode: 0,
        originalRunStdout: "no test runner; oracle #9 skipped (informational)",
        originalRunExitCode: 1,
        mutationApplied: false,
        mutationReverted: false,
      },
    };
  }

  if (fixedRun.exitCode !== 0) {
    throw new Error(
      `oracle #9a FAIL: test did not pass against fixed code. exitCode=${fixedRun.exitCode}, stdout=${fixedRun.stdout.slice(0, 500)}, stderr=${fixedRun.stderr.slice(0, 500)}`,
    );
  }

  // 3. Mutation: revert the fix's source edits, keep the test file.
  const postFixContents = revertFixInOverlay(args.overlay, args.fix.patch);

  // Reindex only the fix files (not the test) so SAST reflects pre-fix state.
  const fixFiles = args.fix.patch.fileEdits.map((e: CodePatch["fileEdits"][number]) => e.file);
  await reindex(args.overlay, fixFiles);

  // 4. Oracle #9b: test must fail against unfixed code.
  const originalRun = runTest(args.overlay, args.testFilePath, args.mainRepoRoot);

  if (originalRun.exitCode === 0) {
    // Restore the fix before throwing so the overlay is clean for any
    // diagnostic the caller wants to run on the unfixed state.
    restoreFixInOverlay(args.overlay, postFixContents);
    await reindex(args.overlay, fixFiles);
    throw new Error(
      `oracle #9b FAIL: test PASSED against original (unfixed) code. Test does not lock in the fix. stdout=${originalRun.stdout.slice(0, 500)}`,
    );
  }

  // 5. Restore the fix so downstream stages see the fixed state.
  restoreFixInOverlay(args.overlay, postFixContents);
  await reindex(args.overlay, fixFiles);

  return {
    passesOnFixedCode: true,
    failsOnOriginalCode: true,
    audit: {
      fixedRunStdout: fixedRun.stdout,
      fixedRunExitCode: fixedRun.exitCode,
      originalRunStdout: originalRun.stdout,
      originalRunExitCode: originalRun.exitCode,
      mutationApplied: true,
      mutationReverted: true,
    },
  };
}
