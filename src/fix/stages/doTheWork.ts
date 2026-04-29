/**
 * The unified do-the-work stage. One LLM call produces patch + test as a
 * single work product; the verification gates (Oracle #2, Oracle #9) stay
 * separate and run mechanically after the LLM returns.
 *
 * Architectural rationale: see docs/specs/2026-04-27-constraint-driven-development.md
 * §"One fork: the verifier's verdict" and the do-the-work prompt at
 * src/fix/prompts/doTheWork.ts. The framework operates on intents; "fix this
 * bug" and "add this feature" are the same shape (intent → change + test
 * that locks the change in). Splitting patch-gen from test-gen drifts the
 * test away from the patch's intent and produces placebo tests. Holistic
 * generation keeps the test honest about the patch.
 *
 * v1 scope: this stage handles the LLM call and the patch-vs-test split.
 * It returns the captured outputs; the caller (orchestrator) runs Oracle #2
 * via verifyCandidate() and Oracle #9 via the existing mutation-verification
 * helper. That keeps the verification machinery untouched while collapsing
 * the generation.
 *
 * Invocation today is opt-in. Set args.useUnifiedAgent = true on the
 * orchestrator entry, or call doTheWork() directly. The legacy
 * generateFixCandidate + generateRegressionTest path remains the default
 * until Oracle #9's mutation verification is factored into a reusable
 * helper that this stage can call directly.
 */

import { join, basename } from "path";
import type {
  IntentSignal,
  BugLocus,
  InvariantClaim,
  CodePatch,
  OverlayHandle,
  LLMProvider,
  FixCandidate,
} from "../types.js";
import type { FixLoopLogger } from "../logger.js";
import { runAgentInOverlay } from "../captureChange.js";
import { verifyCandidate } from "../candidateGen.js";
import { getModelTier } from "../modelTiers.js";
import { getPromptStore } from "../../llm/promptStore.js";
import {
  DO_THE_WORK_PROMPT_TEMPLATE,
  DO_THE_WORK_PROMPT_DISCRIMINATOR,
} from "../prompts/doTheWork.js";
import { getIntentText } from "../types.js";

/**
 * Output of doTheWork. The patch and test are both captured from a single
 * agent invocation; the verifier-side fields (invariantHoldsUnderOverlay,
 * overlayZ3Verdict) come from the same Oracle #2 path generateFixCandidate
 * uses.
 */
export interface DoTheWorkResult {
  fix: FixCandidate;
  /** The captured test file's path, repo-relative inside the overlay. */
  testFilePath: string;
  /** The captured test file's contents, ready to write back to the overlay. */
  testCode: string;
  /** The LLM's rationale (final text block from the agent run). */
  rationale: string;
  /** Number of agent turns used. */
  turnsUsed: number;
}

export interface DoTheWorkArgs {
  signal: IntentSignal;
  locus: BugLocus;
  invariant: InvariantClaim;
  overlay: OverlayHandle;
  llm: LLMProvider;
  /** Investigate's findings, threaded through the prompt. */
  investigateReport?: import("./investigate.js").InvestigateReport;
  /** Host project root, optional. When provided, the prompt routes through bp. */
  projectRoot?: string;
  logger?: FixLoopLogger;
}

/**
 * Run the unified do-the-work stage. Caller is responsible for verifying
 * the returned test via Oracle #9 (mutation verification).
 */
export async function doTheWork(args: DoTheWorkArgs): Promise<DoTheWorkResult> {
  if (!args.llm.agent) {
    throw new Error(
      "doTheWork: LLM provider does not implement agent() — cannot run unified stage",
    );
  }

  const prompt = await buildDoTheWorkPrompt(args);

  const { patch: rawCapture, rationale, turnsUsed } = await runAgentInOverlay({
    overlay: args.overlay,
    llm: args.llm,
    prompt,
    logger: args.logger,
    model: getModelTier("C3-agent"),
    stage: "do-the-work",
  });

  const { sourceEdits, testEdits } = splitCapturedEdits(rawCapture);

  if (testEdits.length === 0) {
    throw new Error(
      "doTheWork: agent produced no test file. The unified prompt requires both a patch AND a test that locks it in. " +
        `Captured ${sourceEdits.length} source edit(s); zero test files.`,
    );
  }

  if (testEdits.length > 1) {
    // Multiple test files captured. v1 picks the first; future versions may
    // emit a multi-test artifact. Surface the ambiguity rather than swallowing.
    args.logger?.info(
      `doTheWork: agent produced ${testEdits.length} test files; using the first (${testEdits[0].file}). ` +
        `Other test files: ${testEdits.slice(1).map((e) => e.file).join(", ")}.`,
    );
  }

  const sourcePatch: CodePatch = {
    fileEdits: sourceEdits,
    description:
      `unified do-the-work agent (${sourceEdits.length} source edit${sourceEdits.length === 1 ? "" : "s"})`,
  };

  // Run Oracle #2 against the source patch. verifyCandidate applies the
  // patch idempotently (it's already on disk in the overlay; the call just
  // reconfirms via reindex + Z3).
  const proposed = { patch: sourcePatch, rationale, confidence: 1.0 };
  const oracleTwo = await verifyCandidate(proposed, args.overlay, args.invariant);

  const fix: FixCandidate = {
    patch: sourcePatch,
    llmRationale: rationale,
    llmConfidence: 1.0,
    invariantHoldsUnderOverlay: oracleTwo.invariantHoldsUnderOverlay,
    overlayZ3Verdict: oracleTwo.z3Verdict,
    audit: oracleTwo.audit,
  };

  return {
    fix,
    testFilePath: testEdits[0].file,
    testCode: testEdits[0].newContent,
    rationale,
    turnsUsed,
  };
}

/**
 * Split the agent's captured CodePatch into source edits (the patch) and
 * test file edits (the regression test). Heuristic: any path matching
 * `*.test.ts`, `*.test.tsx`, `*.spec.ts`, or under a `__tests__/` segment is
 * a test file. Everything else is a source edit.
 *
 * The heuristic is intentionally permissive — agents may pick non-canonical
 * test paths and the do-the-work prompt encourages a co-located
 * `<file>.regression.test.ts` shape. Anything matching the pattern is
 * routed to the test artifact.
 */
export function splitCapturedEdits(patch: CodePatch): {
  sourceEdits: CodePatch["fileEdits"];
  testEdits: CodePatch["fileEdits"];
} {
  const sourceEdits: CodePatch["fileEdits"] = [];
  const testEdits: CodePatch["fileEdits"] = [];
  for (const edit of patch.fileEdits) {
    if (isTestPath(edit.file)) {
      testEdits.push(edit);
    } else {
      sourceEdits.push(edit);
    }
  }
  return { sourceEdits, testEdits };
}

function isTestPath(path: string): boolean {
  const name = basename(path);
  if (/\.(test|spec)\.(ts|tsx|js|mjs|cjs)$/.test(name)) return true;
  if (path.includes("/__tests__/") || path.startsWith("__tests__/")) return true;
  if (path.includes("/tests/") || path.startsWith("tests/")) return true;
  return false;
}

async function buildDoTheWorkPrompt(args: DoTheWorkArgs): Promise<string> {
  let body = DO_THE_WORK_PROMPT_TEMPLATE;
  if (args.projectRoot) {
    const rev = await getPromptStore(args.projectRoot).get(
      "do-the-work.prompt",
      DO_THE_WORK_PROMPT_TEMPLATE,
      DO_THE_WORK_PROMPT_DISCRIMINATOR,
    );
    body = rev.body;
  }

  const intentSection = `Intent summary: ${args.signal.summary}
User text: ${getIntentText(args.signal)}`;

  const investigateBlock = args.investigateReport
    ? renderInvestigateBlock(args.investigateReport, args.locus)
    : renderLocusFallback(args.locus, args.invariant);

  // The locus display is the relative path the agent should read first.
  const locusDisplay = args.locus.file;

  return body
    .replaceAll("{{INTENT_SECTION}}", intentSection)
    .replaceAll("{{INVESTIGATE_BLOCK}}", investigateBlock)
    .replaceAll("{{LOCUS_DISPLAY}}", locusDisplay);
}

function renderInvestigateBlock(
  report: import("./investigate.js").InvestigateReport,
  locus: BugLocus,
): string {
  const primary = report.primaryLocation;
  return `

# Where investigation pointed

Primary location: ${primary.file}${primary.function ? ` (${primary.function})` : ""}${primary.lineRange ? ` lines ${primary.lineRange[0]}-${primary.lineRange[1]}` : ""}
  Rationale: ${primary.rationale}
- Root-cause hypothesis: ${report.rootCauseHypothesis}
- Fix hypothesis: ${report.fixHypothesis}
- Locate confirmed: ${locus.file}:${locus.line}${locus.function ? ` (${locus.function})` : ""}
- Other candidates Investigate considered: ${report.candidateLocations.length}
`;
}

function renderLocusFallback(locus: BugLocus, invariant: InvariantClaim): string {
  return `

# Where to look

Locate identified: ${locus.file}:${locus.line}${locus.function ? ` (${locus.function})` : ""}
The formal invariant: ${invariant.description}
SMT (violation state — must become unsat after your patch):
${invariant.formalExpression}
`;
}
