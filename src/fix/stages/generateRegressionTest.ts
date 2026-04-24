/**
 * C5: Regression test generator with mutation verification (oracle #9).
 *
 * Produces a TestArtifact that is:
 *   - Verified to PASS against the fixed code (oracle #9a)
 *   - Verified to FAIL against the original code after reverting the fix (oracle #9b)
 *
 * Both directions are required. A test that only passes after the fix proves
 * nothing. A test that fails after reverting proves the test locks in the fix.
 */

import { applyPatchToOverlay, reindexOverlay } from "../overlay.js";
import {
  extractWitnessInputs,
  generateTestCode,
  chooseTestFilePath,
  runTestInOverlay,
  revertFixInOverlay,
  restoreFixInOverlay,
  resolveMainRepoRoot,
} from "../testGen.js";
import type {
  FixCandidate,
  BugSignal,
  BugLocus,
  OverlayHandle,
  TestArtifact,
  LLMProvider,
  InvariantClaim,
} from "../types.js";

export async function generateRegressionTest(args: {
  fix: FixCandidate;
  signal: BugSignal;
  locus: BugLocus;
  overlay: OverlayHandle;
  invariant: InvariantClaim;
  llm: LLMProvider;
  /**
   * Injectable test runner for the overlay. When provided, replaces real vitest
   * execution (for integration tests that must not spawn vitest-inside-vitest).
   * Receives the overlay and test file path; returns { exitCode, stdout, stderr }.
   */
  testRunner?: (overlay: OverlayHandle, testFilePath: string, mainRepoRoot: string) => { exitCode: number; stdout: string; stderr: string };
}): Promise<TestArtifact> {
  const { fix, signal, locus, overlay, invariant, llm } = args;

  // -------------------------------------------------------------------------
  // Step 1: Extract Z3 witness as JS values
  // -------------------------------------------------------------------------
  const witnessInputs = extractWitnessInputs(invariant);

  // -------------------------------------------------------------------------
  // Step 2: Derive test file path and name
  // -------------------------------------------------------------------------
  const testFilePath = chooseTestFilePath(locus, overlay);
  const testName = `regression: ${signal.summary.slice(0, 80)}`;

  // -------------------------------------------------------------------------
  // Step 3: LLM generates a vitest test using those inputs
  // -------------------------------------------------------------------------
  const testCode = await generateTestCode({
    signal,
    locus,
    invariant,
    inputs: witnessInputs,
    testFilePath,
    testName,
    llm,
    overlay,
  });

  // -------------------------------------------------------------------------
  // Step 4: Write the test file into the overlay (C3's fix is already applied)
  // -------------------------------------------------------------------------
  applyPatchToOverlay(overlay, {
    fileEdits: [{ file: testFilePath, newContent: testCode }],
    description: "regression test (C5)",
  });

  // -------------------------------------------------------------------------
  // Step 5: Oracle #9a — run test against FIXED code
  // -------------------------------------------------------------------------
  const mainRepoRoot = resolveMainRepoRoot(overlay);
  const runTest = args.testRunner ?? runTestInOverlay;
  const fixedRun = runTest(overlay, testFilePath, mainRepoRoot);

  if (fixedRun.exitCode !== 0) {
    throw new Error(
      `oracle #9a FAIL: test did not pass against fixed code. exitCode=${fixedRun.exitCode}, stdout=${fixedRun.stdout.slice(0, 500)}`,
    );
  }

  // -------------------------------------------------------------------------
  // Step 6: Mutation — revert the fix, keep the test file
  // -------------------------------------------------------------------------
  const postFixContents = revertFixInOverlay(overlay, fix.patch);

  // Reindex only the fix files (not the test file) so SAST reflects pre-fix state
  const fixFiles = fix.patch.fileEdits.map((e) => e.file);
  await reindexOverlay(overlay, fixFiles);

  // -------------------------------------------------------------------------
  // Step 7: Oracle #9b — run test against ORIGINAL (unfixed) code
  // -------------------------------------------------------------------------
  const originalRun = runTest(overlay, testFilePath, mainRepoRoot);

  if (originalRun.exitCode === 0) {
    // Test passed against unfixed code — not mutation-verified.
    // Restore the fix before throwing.
    restoreFixInOverlay(overlay, postFixContents);
    await reindexOverlay(overlay, fixFiles);
    throw new Error(
      `oracle #9b FAIL: test PASSED against original (unfixed) code. Test does not lock in the fix. stdout=${originalRun.stdout.slice(0, 500)}`,
    );
  }

  // -------------------------------------------------------------------------
  // Step 8: Restore the fix so downstream stages see the fixed state
  // -------------------------------------------------------------------------
  restoreFixInOverlay(overlay, postFixContents);
  await reindexOverlay(overlay, fixFiles);

  // -------------------------------------------------------------------------
  // Step 9: Return artifact with full audit
  // -------------------------------------------------------------------------
  return {
    testFilePath,
    testName,
    testCode,
    witnessInputs,
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
