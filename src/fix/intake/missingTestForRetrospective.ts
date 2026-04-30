/**
 * B0 retrospective: missing-test generation for the IntentReport output bundle.
 *
 * When retrospective intake (extractIntent) reads a commit and identifies an
 * intent that ships without a regression test (hasRegressionTest: false AND
 * testGenerationOpportunity: true), the pipeline should produce the missing
 * test as part of the report's outputBundle. This module is the wiring from
 * B0 (retrospective.ts) to C5 (the prospective stage's test-generation
 * surface).
 *
 * It does NOT write the test file to disk on the user's repo. Per the
 * standing-invariant-runtime spec (§ "Intake unification (v1)", outputBundle
 * schema lines 101-105), generated tests live as strings in
 * `IntentReport.outputBundle.addedTests`. Each entry is the FULL test file
 * contents. Where they actually land on disk is a downstream consumer
 * decision (e.g. `provekit mine-history --apply`, future fix-loop bundle
 * assembly). This module's only side effect is the temporary git overlay
 * worktree it opens to drive C5's agent + reads back the produced test code.
 *
 * Reference: protocol/specs/2026-04-27-standing-invariant-runtime.md
 *            (Intake unification, outputBundle.addedTests)
 *            protocol/specs/2026-04-27-constraint-driven-development.md
 *            (the retrospective intake direction)
 *
 * Pipeline shape:
 *   IntentReport (in)
 *     │
 *     ▼
 *   for each intent where hasRegressionTest=false ∧
 *                        testGenerationOpportunity=true ∧
 *                        constraintCandidate ≠ null:
 *     │
 *     ▼   synthesize BugSignal / BugLocus / InvariantClaim
 *     │   open overlay rooted at projectRoot HEAD
 *     │   chooseTestFilePath(locus, overlay)
 *     │   generateTestCodeViaAgent(...)  — agent writes file inside overlay
 *     │   readFileSync(overlay/testFilePath)
 *     │   closeOverlay
 *     │
 *     ▼
 *   IntentReport with outputBundle.addedTests appended (out)
 *
 * Per-intent failures are logged and isolated: a single bad intent does not
 * abort the rest of the report.
 */

import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { openDb } from "../../db/index.js";
import { createScratchDir } from "../scratchDir.js";
import { closeOverlay } from "../overlay.js";
import { openOverlay } from "../stages/openOverlay.js";
import { chooseTestFilePath, generateTestCodeViaAgent } from "../testGen.js";
import type {
  BugLocus,
  BugSignal,
  InvariantClaim,
  LLMProvider,
} from "../types.js";
import type {
  IntentReport,
  IntentReportIntent,
  IntentReportConstraintCandidate,
} from "./retrospective.js";
import { validateIntentReport } from "../../contracts/intentReport.js";

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/**
 * For each intent in the report that is (a) testable, (b) shipping without a
 * regression test, and (c) carries a constraint candidate, drive C5's agent
 * to synthesize a test. Append each produced test (full file contents) to
 * `report.outputBundle.addedTests` and return the augmented report.
 *
 * Errors per intent are caught and logged via console.error; the overall
 * walk does not abort. This matches mine-history's per-commit error
 * tolerance: opportunistic test generation should never poison the rest of
 * the report.
 *
 * @param report       The IntentReport produced by extractIntent.
 * @param llm          LLMProvider. C5's agent path requires `llm.agent`. If
 *                     `llm.agent` is undefined, the function logs and skips
 *                     test generation entirely (returns the report unchanged).
 * @param projectRoot  Absolute path to the git repo. The overlay is rooted
 *                     against the file's enclosing repo HEAD; the standing
 *                     runtime re-resolves bindings at verify time.
 */
export async function generateMissingTestsForReport(args: {
  report: IntentReport;
  llm: LLMProvider;
  projectRoot: string;
}): Promise<IntentReport> {
  const { report, llm, projectRoot } = args;

  // Contract gate: the upstream report must be well-formed before we touch it.
  validateIntentReport(report);

  // C5's preferred path is the agent path (generateTestCodeViaAgent). If the
  // provider has no agent surface, the agent-driven prompt cannot fire — and
  // the non-agent path (generateTestCode) lacks the strong "force-the-precondition"
  // teaching that makes mutation verification stable. Skip with a single log
  // rather than producing weak placebo tests.
  if (!llm.agent) {
    console.error(
      "generateMissingTestsForReport: llm.agent is undefined; skipping " +
        "missing-test generation. (C5's agent path is required for the " +
        "retrospective output-bundle test-generation step.)",
    );
    return report;
  }

  const addedTests: string[] = [...report.outputBundle.addedTests];

  for (const intent of report.intents) {
    if (!shouldGenerate(intent)) continue;
    // narrow constraintCandidate (shouldGenerate checked it)
    const candidate = intent.constraintCandidate!;

    try {
      const testCode = await generateOneMissingTest({
        intent,
        candidate,
        report,
        llm,
        projectRoot,
      });
      addedTests.push(testCode);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error(
        `generateMissingTestsForReport: intent at ${intent.filePath}:` +
          `${intent.lineRange[0]} skipped due to error: ${msg}`,
      );
      // Continue with the next intent; one bad intent must not abort the
      // whole report.
    }
  }

  const augmented: IntentReport = {
    ...report,
    outputBundle: {
      ...report.outputBundle,
      addedTests,
    },
  };

  return validateIntentReport(augmented);
}

// ---------------------------------------------------------------------------
// Per-intent: synthesize C5 inputs, drive the agent, read the test code back
// ---------------------------------------------------------------------------

/**
 * Drive C5 once for a single intent. Throws on any failure (caller catches
 * + continues with the next intent).
 *
 * Input synthesis notes (these are the "spec gaps" the task asked to fill in):
 *
 * - BugSignal.source = "retrospective-intake" — intake-source string,
 *   not a closed enum (BugSignal.source is `string` per types.ts).
 * - BugLocus.file is set to the ABSOLUTE path of the file in the user's
 *   repo (projectRoot + intent.filePath). chooseTestFilePath needs this
 *   absolute → it computes `relative(overlay.worktreePath, locus.file)`
 *   and falls back to a basename-only path when the file is not under the
 *   overlay. Since the overlay is a git worktree of the same repo, the
 *   file's repo-relative path matches inside the overlay too — so we point
 *   the locus at the overlay copy of the file, which yields the correct
 *   `src/.../foo.regression.test.ts` shape.
 * - BugLocus SAST fields (primaryNode, containingFunction, …) are stubbed
 *   "" / [] — same defaults persistIntent uses in cli.mineHistory.ts. The
 *   standing runtime's path enumerator re-resolves at verify time against
 *   the current substrate; nothing in C5's agent path reads these fields.
 * - InvariantClaim.formalExpression = candidate.smtSketch.
 * - InvariantClaim.llmKind = candidate.kind (downstream classifier prefers
 *   this over keyword heuristics; matches persistIntent).
 * - InvariantClaim.bindings = []  — no SMT-constant → AST-node binding
 *   resolved at intake; binding-resolver runs at verify time.
 * - InvariantClaim.complexity = 0 — no proof complexity computed at intake.
 * - InvariantClaim.witness = null  — no concrete Z3 witness at intake;
 *   abstract invariant. extractWitnessInputs handles witness=null by
 *   returning {} (the agent path supports the empty-input contract via
 *   "(none — abstract invariant; ...)" branch).
 * - witnessInputs is therefore {} — passed explicitly so we don't depend on
 *   the abstract-witness path inside C5.
 */
async function generateOneMissingTest(args: {
  intent: IntentReportIntent;
  candidate: IntentReportConstraintCandidate;
  report: IntentReport;
  llm: LLMProvider;
  projectRoot: string;
}): Promise<string> {
  const { intent, candidate, report, llm, projectRoot } = args;

  // Resolve absolute path to the file in the user's repo. openOverlay reads
  // dirname(locus.file) to find the enclosing git repo, so we MUST give it
  // an absolute path that exists at HEAD.
  const absFilePath = join(projectRoot, intent.filePath);
  if (!existsSync(absFilePath)) {
    throw new Error(
      `intent.filePath does not exist at HEAD: ${intent.filePath} ` +
        `(resolved to ${absFilePath}). Cannot open overlay for missing-test ` +
        `generation.`,
    );
  }

  // Synthesize the BugSignal C5 ingests. summary / failureDescription are
  // the LLM agent's narrative anchor; codeReferences seeds the test's
  // import-target hint. fixHint omitted (intentionally no fix to apply).
  const signal: BugSignal = {
    source: "retrospective-intake",
    rawText:
      `commit ${report.trigger.ref}\n${report.trigger.commitMessage ?? ""}`.trim(),
    summary: `${report.trigger.ref} · ${intent.intent}`,
    failureDescription: intent.intent,
    codeReferences: [
      {
        file: intent.filePath,
        line: intent.lineRange[0],
      },
    ],
  };

  // Open a temporary scratch SAST DB. openOverlay requires a Db handle even
  // though its body never reads from it (vestigial wiring); we open a
  // throwaway db inside a scratch dir and close it at the end alongside
  // the overlay.
  const scratchRoot = createScratchDir("provekit-missing-test-");
  const scratchDbPath = join(scratchRoot, "missing-test.db");
  const scratchDb = openDb(scratchDbPath);

  // Compose the BugLocus pointing at the absolute file path. SAST-structural
  // fields are stubbed; nothing in chooseTestFilePath / generateTestCodeViaAgent
  // depends on them.
  const locusForOverlay: BugLocus = {
    file: absFilePath,
    line: intent.lineRange[0],
    confidence: 0.5,
    primaryNode: "",
    containingFunction: "",
    relatedFunctions: [],
    dataFlowAncestors: [],
    dataFlowDescendants: [],
    dominanceRegion: [],
    postDominanceRegion: [],
  };

  const overlay = await openOverlay({ locus: locusForOverlay, db: scratchDb });

  try {
    // Re-aim locus at the overlay copy of the file so chooseTestFilePath
    // computes a clean repo-relative test path under the overlay.
    const overlayFilePath = join(overlay.worktreePath, intent.filePath);
    const locusInOverlay: BugLocus = {
      ...locusForOverlay,
      file: overlayFilePath,
    };

    const testFilePath = chooseTestFilePath(locusInOverlay, overlay);
    const testName = `regression: ${intent.intent.slice(0, 80)}`;

    // Synthesize the InvariantClaim. The retrospective IntentReport already
    // carries the SMT sketch + kind; everything else has intake-time defaults
    // (witness=null is the abstract case C5 supports natively).
    const invariant: InvariantClaim = {
      principleId: null,
      description: intent.intent,
      formalExpression: candidate.smtSketch,
      bindings: [],
      complexity: 0,
      witness: null,
      citations: (intent.citations ?? []).map((c) => ({
        smt_clause: c.smtClause,
        source_quote: c.sourceQuote,
      })),
      llmKind: candidate.kind,
    };

    // Empty witness inputs: abstract invariant, no concrete Z3 witness yet.
    // generateTestCodeViaAgent prompt has explicit handling for this case
    // ("(none — abstract invariant; derive test inputs from the bug summary
    // and invariant description below)").
    const witnessInputs: Record<string, unknown> = {};

    await generateTestCodeViaAgent({
      signal,
      locus: locusInOverlay,
      invariant,
      inputs: witnessInputs,
      testFilePath,
      testName,
      llm,
      overlay,
    });

    // Read the agent-written test file out of the overlay BEFORE closing
    // (closeOverlay removes the worktree). The agent path validates the
    // file exists + has it()/vitest imports inside; if any of those fail
    // it throws before reaching here.
    const absTestPath = join(overlay.worktreePath, testFilePath);
    const testCode = readFileSync(absTestPath, "utf8");

    return testCode;
  } finally {
    // Close the overlay (removes scratch worktree, closes scratch SAST db
    // handle, deletes db file). Best-effort: errors here would leak a
    // scratch dir but must not mask a real test-generation error.
    try {
      await closeOverlay(overlay);
    } catch {
      // Best-effort cleanup.
    }
  }
}

// ---------------------------------------------------------------------------
// Predicate: should this intent get a generated test?
// ---------------------------------------------------------------------------

/**
 * The three conditions for missing-test generation, all required:
 *   1. hasRegressionTest === false  — the diff itself didn't lock in the property.
 *   2. testGenerationOpportunity === true  — the property is testable in
 *      principle (the LLM signals untestable cases like dependency bumps).
 *   3. constraintCandidate !== null  — without an SMT sketch + kind we have
 *      nothing concrete enough to drive a test against.
 */
function shouldGenerate(intent: IntentReportIntent): boolean {
  return (
    intent.hasRegressionTest === false &&
    intent.testGenerationOpportunity === true &&
    intent.constraintCandidate !== null
  );
}
