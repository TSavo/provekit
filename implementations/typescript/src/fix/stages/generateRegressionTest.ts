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

import {
  applyPatchToOverlay as defaultApplyPatch,
  reindexOverlay as defaultReindex,
} from "../overlay.js";
import {
  extractWitnessInputs,
  generateTestCode,
  generateTestCodeViaAgent,
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
  CodePatch,
} from "../types.js";
import type { FixLoopLogger } from "../logger.js";
import { instantiateTestTemplate } from "./recognizeTemplates.js";
import type { RecognizeResult } from "./recognize.js";
import { pickPrimaryPatchFile } from "../runtime/patchUtils.js";
import { verifyOracle9 } from "../runtime/oracle9.js";

/**
 * Stage dependencies for C5. Defaults preserve current behavior; override
 * to inject a different Sandbox/PatchApplicator without touching the
 * regression-test logic.
 */
export interface GenerateRegressionTestDeps {
  applyPatch?: (overlay: OverlayHandle, patch: CodePatch) => void | Promise<void>;
  reindex?: (overlay: OverlayHandle, files: string[]) => Promise<void>;
}

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
  logger?: FixLoopLogger;
  /** B3 mechanical-mode input. When matched, C5m runs (no LLM). */
  recognized?: RecognizeResult;
  /**
   * Investigate's report when symptom-only flow fired. C5 cites
   * `rootCauseHypothesis` and `fixHypothesis` so the regression test
   * reproduces the bug at the scale the symptom describes — placebo
   * tests at small scale that "pass against the buggy code" are the
   * #1 way oracle #9a fails on real-world dogfoods.
   */
  investigateReport?: import("./investigate.js").InvestigateReport;
  /**
   * Host project root, optional. Threaded into generateTestCodeViaAgent
   * so the C5 prompt fragment resolves via better-prompts.
   */
  projectRoot?: string;
  /** Optional dependency injection seams; falls back to module defaults. */
  deps?: GenerateRegressionTestDeps;
}): Promise<TestArtifact> {
  const { fix, signal, locus, overlay, invariant, llm } = args;
  const applyPatch = args.deps?.applyPatch ?? defaultApplyPatch;
  const reindex = args.deps?.reindex ?? defaultReindex;

  // -------------------------------------------------------------------------
  // Step 1: Extract Z3 witness as JS values
  // -------------------------------------------------------------------------
  const witnessInputs = extractWitnessInputs(invariant);

  // -------------------------------------------------------------------------
  // Step 2: Derive test file path and name
  // -------------------------------------------------------------------------
  let testFilePath: string;
  const testName = `regression: ${signal.summary.slice(0, 80)}`;
  let testCode: string;
  let source: "library" | "llm" = "llm";

  // -------------------------------------------------------------------------
  // C5m: B3 recognized path. Mechanical instantiation of testTemplate.
  // -------------------------------------------------------------------------
  if (args.recognized && args.recognized.matched && args.recognized.principle.testTemplate) {
    const inst = instantiateTestTemplate({
      template: args.recognized.principle.testTemplate,
      locus,
      overlay,
      bindings: args.recognized.bindings,
      witnessInputs,
    });
    testFilePath = inst.testFilePath;
    testCode = inst.testCode;
    source = "library";
  } else {
    // C3's fix.patch.fileEdits is the authoritative "where the bug actually
    // lives." Locate sometimes resolves to an entry-point or wrapper that
    // doesn't host the bug (an API route that calls into the real data
    // layer); C3 already adjudicated and may have patched a different file
    // entirely. The regression test must aim at the same surface C3 patched
    // — testing the wrapper invites the agent to mock the data layer and
    // produce a placebo. Use the (largest, by edit length) patched file as
    // the locus for the test path; fall back to Locate's locus if the
    // patch list is empty.
    const patchLocusPath = pickPrimaryPatchFile(args.fix);
    const testLocus: BugLocus = patchLocusPath
      ? { ...locus, file: patchLocusPath }
      : locus;
    testFilePath = chooseTestFilePath(testLocus, overlay);
    // ---------------------------------------------------------------------
    // Step 3: LLM generates a vitest test using those inputs.
    // ---------------------------------------------------------------------
    testCode = llm.agent
      ? await generateTestCodeViaAgent({
          signal,
          locus: testLocus,
          invariant,
          inputs: witnessInputs,
          testFilePath,
          testName,
          llm,
          overlay,
          investigateReport: args.investigateReport,
          projectRoot: args.projectRoot,
        })
      : await generateTestCode({
          signal,
          locus: testLocus,
          invariant,
          inputs: witnessInputs,
          testFilePath,
          testName,
          llm,
          overlay,
          projectRoot: args.projectRoot,
        });
  }

  // -------------------------------------------------------------------------
  // Step 4-9: Oracle #9 mutation verification — write test, run against
  // fixed code (#9a), revert fix and run against original (#9b), restore.
  // The mechanism is shared with the unified doTheWork stage; both call
  // the same helper.
  // -------------------------------------------------------------------------
  const mainRepoRoot = resolveMainRepoRoot(overlay);
  const result = await verifyOracle9({
    overlay,
    fix,
    testFilePath,
    testCode,
    mainRepoRoot,
    testRunner: args.testRunner,
    applyPatch,
    reindex,
  });

  return {
    source,
    testFilePath,
    testName,
    testCode,
    witnessInputs,
    passesOnFixedCode: result.passesOnFixedCode,
    failsOnOriginalCode: result.failsOnOriginalCode,
    audit: result.audit,
  };
}

// pickPrimaryPatchFile lives in ../runtime/patchUtils.ts so the orchestrator's
// invariant-persistence path can share the same patch-file selection logic.
