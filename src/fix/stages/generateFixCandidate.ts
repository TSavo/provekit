/**
 * C3: Fix candidate generator.
 *
 * Given a BugSignal, BugLocus, InvariantClaim, and an open OverlayHandle,
 * proposes up to N code patches via LLM and verifies each by running oracle #2
 * (Z3 under the overlay). Returns the first candidate that passes.
 *
 * Oracle #2 contract: a returned FixCandidate has invariantHoldsUnderOverlay: true
 * ONLY if Z3 confirmed unsat (or the bug site was structurally removed — also
 * mapped to "unsat" in overlayZ3Verdict).
 *
 * Oracles #8 (gap detector) and #10 (full test suite) are deferred to D1.
 * C3 only runs oracle #2.
 *
 * The overlay lifecycle is owned by the orchestrator. C3 does NOT close the overlay.
 */

import type {
  BugSignal,
  BugLocus,
  InvariantClaim,
  OverlayHandle,
  FixCandidate,
  LLMProvider,
  CodePatch,
} from "../types.js";
import type { FixLoopLogger } from "../logger.js";
import { buildFixPrompt, parseProposedFixes, verifyCandidate, buildAgentFixPrompt } from "../candidateGen.js";
import { runAgentInOverlay } from "../captureChange.js";
import { requestStructuredJson } from "../llm/structuredOutput.js";
import { getPromptStore } from "../../llm/promptStore.js";
import { getModelTier } from "../modelTiers.js";
import { instantiateFixTemplate } from "./recognizeTemplates.js";
import type { RecognizeResult } from "./recognize.js";
import {
  applyPatchToOverlay as defaultApplyPatch,
  reindexOverlay as defaultReindex,
} from "../overlay.js";
import { createNoopLogger } from "../logger.js";

/**
 * Stage dependencies for C3.
 *
 * The orchestrator and tests can override these to plug in a different
 * Sandbox/PatchApplicator without touching the C3 logic. Defaults preserve
 * the current behavior (applyPatchToOverlay + reindexOverlay).
 */
export interface GenerateFixCandidateDeps {
  /** Apply the patch to the overlay tree. Default: applyPatchToOverlay. */
  applyPatch?: (overlay: OverlayHandle, patch: CodePatch) => void | Promise<void>;
  /** Re-index the overlay's SAST DB after a patch lands. Default: reindexOverlay. */
  reindex?: (overlay: OverlayHandle, files: string[]) => Promise<void>;
}

export async function generateFixCandidate(args: {
  signal: BugSignal;
  locus: BugLocus;
  invariant: InvariantClaim;
  overlay: OverlayHandle;
  llm: LLMProvider;
  options?: { maxCandidates?: number; minConfidence?: number };
  logger?: FixLoopLogger;
  /** B3 mechanical-mode input. When matched, C3m runs (no LLM). */
  recognized?: RecognizeResult;
  /**
   * Investigate's report when symptom-only flow fired. Carried through to
   * buildAgentFixPrompt so C3's reasoning shows the upstream chain
   * (primary location, root-cause hypothesis, fix hypothesis) and the
   * LLM patches at the locus rather than wandering up the call stack.
   */
  investigateReport?: import("./investigate.js").InvestigateReport;
  /**
   * Host project root, optional. When provided, the C3 prompt fragment
   * resolves via better-prompts (c3.agent_fix_prompt artifact). Day 0
   * byte-identical; bp.evolve later returns evolved revisions.
   */
  projectRoot?: string;
  /** Optional dependency injection seams; falls back to module defaults. */
  deps?: GenerateFixCandidateDeps;
}): Promise<FixCandidate> {
  // C3m: B3 recognized path. Mechanical instantiation of fixTemplate.
  if (args.recognized && args.recognized.matched) {
    return generateFixCandidateViaLibrary({
      recognized: args.recognized,
      locus: args.locus,
      invariant: args.invariant,
      overlay: args.overlay,
      logger: args.logger,
      deps: args.deps,
    });
  }
  // Agent path: if the LLM provider supports agent(), use capture-the-change.
  if (args.llm.agent) {
    return generateFixCandidateViaAgent({ ...args, logger: args.logger });
  }
  // Legacy JSON-patch path.
  return generateFixCandidateViaJson(args);
}

// ---------------------------------------------------------------------------
// C3m: library-mode mechanical fix.
// ---------------------------------------------------------------------------

async function generateFixCandidateViaLibrary(args: {
  recognized: Extract<RecognizeResult, { matched: true }>;
  locus: BugLocus;
  invariant: InvariantClaim;
  overlay: OverlayHandle;
  logger?: FixLoopLogger;
  deps?: GenerateFixCandidateDeps;
}): Promise<FixCandidate> {
  const logger = args.logger ?? createNoopLogger();
  const { recognized, locus, invariant, overlay } = args;
  const fixTemplate = recognized.principle.fixTemplate!;
  const applyPatch = args.deps?.applyPatch ?? defaultApplyPatch;
  const reindex = args.deps?.reindex ?? defaultReindex;

  const t0 = Date.now();
  const patch: CodePatch = instantiateFixTemplate({
    template: fixTemplate,
    locus,
    overlay,
    bindings: recognized.bindings,
  });

  await applyPatch(overlay, patch);
  await reindex(overlay, patch.fileEdits.map((e) => e.file));

  // Verify via oracle #2 (same as LLM mode). Reuse verifyCandidate.
  const proposed = {
    patch,
    rationale: fixTemplate.rationale,
    confidence: 1.0,
  };
  const result = await verifyCandidate(proposed, overlay, invariant);

  logger.detail(`[C3m] applied library fix for '${recognized.principleId}' in ${Date.now() - t0}ms`);

  return {
    patch,
    source: "library",
    llmRationale: fixTemplate.rationale,
    llmConfidence: 1.0,
    invariantHoldsUnderOverlay: result.invariantHoldsUnderOverlay,
    overlayZ3Verdict: result.z3Verdict,
    audit: result.audit,
  };
}

// ---------------------------------------------------------------------------
// Agent path (new C3 default when provider supports agent())
// ---------------------------------------------------------------------------

async function generateFixCandidateViaAgent(args: {
  signal: BugSignal;
  locus: BugLocus;
  invariant: InvariantClaim;
  overlay: OverlayHandle;
  llm: LLMProvider;
  options?: { maxCandidates?: number; minConfidence?: number };
  logger?: FixLoopLogger;
  investigateReport?: import("./investigate.js").InvestigateReport;
  /**
   * Host project root, optional. When provided, the C3 prompt fragment
   * resolves via better-prompts (c3.agent_fix_prompt artifact, byte-
   * identical day 0). When absent, the literal source-of-record is used
   * directly — same content either way.
   */
  projectRoot?: string;
}): Promise<FixCandidate> {
  const { signal, locus, invariant, overlay } = args;

  const { prompt, revisions: bpRevisions } = await buildAgentFixPrompt(
    signal,
    locus,
    invariant,
    overlay,
    args.investigateReport,
    args.projectRoot,
  );

  // First attempt.
  const { patch, rationale } = await runAgentInOverlay({
    overlay,
    llm: args.llm,
    prompt,
    logger: args.logger,
    model: getModelTier("C3-agent"),
    stage: "C3-agent",
  });

  // bp telemetry: record the C3 invocation against c3.agent_fix_prompt.
  // Signal pass/fail after oracle #2 returns. No-op when projectRoot is
  // absent (bpRevisions is empty in that case).
  const bpInvocationIds: string[] = [];
  if (args.projectRoot && bpRevisions.length > 0) {
    const bp = getPromptStore(args.projectRoot);
    const baseVars = {
      signalSummary: signal.summary,
      locusFile: locus.file,
      invariantDescription: invariant.description,
    };
    const baseMetadata = {
      stage: "C3",
      model: getModelTier("C3-agent"),
      attempt: "first",
      patchFileCount: patch.fileEdits.length,
    };
    for (const { key, revisionId } of bpRevisions) {
      const inv = await bp.record({
        artifactKey: key,
        revisionId,
        vars: baseVars,
        metadata: baseMetadata,
        output: rationale,
      });
      bpInvocationIds.push(inv.id);
    }
  }

  // verifyCandidate applies the patch to the overlay (idempotent rewrite),
  // re-indexes, and runs oracle #2.
  const proposed = { patch, rationale, confidence: 1.0 };
  const result = await verifyCandidate(proposed, overlay, invariant);

  // Signal Oracle #2's verdict on the C3 invocation(s).
  if (bpInvocationIds.length > 0 && args.projectRoot) {
    const bp = getPromptStore(args.projectRoot);
    for (const invId of bpInvocationIds) {
      await bp.signal(invId, {
        verdict: result.invariantHoldsUnderOverlay ? "pass" : "fail",
        reason: result.invariantHoldsUnderOverlay
          ? `oracle #2 — invariant holds under C3's first patch`
          : `oracle #2 — invariant does not hold (z3: ${result.z3Verdict})`,
        source: "c3-oracle-2",
      });
    }
  }

  if (result.invariantHoldsUnderOverlay) {
    return {
      patch,
      llmRationale: rationale,
      llmConfidence: 1.0,
      invariantHoldsUnderOverlay: true,
      overlayZ3Verdict: result.z3Verdict,
      audit: result.audit,
    };
  }

  // ONE retry with feedback about oracle #2 failure.
  const retryPrompt =
    `${prompt}\n\nYour previous fix attempt did not satisfy the invariant. ` +
    `Oracle #2 returned: ${result.z3Verdict}. Please revise the fix.`;

  const { patch: retryPatch, rationale: retryRationale } = await runAgentInOverlay({
    overlay,
    llm: args.llm,
    prompt: retryPrompt,
    logger: args.logger,
    model: getModelTier("C3-agent"),
    stage: "C3-agent",
  });

  const proposed2 = { patch: retryPatch, rationale: retryRationale, confidence: 1.0 };
  const result2 = await verifyCandidate(proposed2, overlay, invariant);

  if (result2.invariantHoldsUnderOverlay) {
    return {
      patch: retryPatch,
      llmRationale: retryRationale,
      llmConfidence: 1.0,
      invariantHoldsUnderOverlay: true,
      overlayZ3Verdict: result2.z3Verdict,
      audit: result2.audit,
    };
  }

  throw new Error(
    `generateFixCandidate (agent path): both attempts failed oracle #2. ` +
    `Invariant '${invariant.description}' could not be satisfied.`,
  );
}

// ---------------------------------------------------------------------------
// Legacy JSON-patch path (backward compat for providers without agent())
// ---------------------------------------------------------------------------

async function generateFixCandidateViaJson(args: {
  signal: BugSignal;
  locus: BugLocus;
  invariant: InvariantClaim;
  overlay: OverlayHandle;
  llm: LLMProvider;
  options?: { maxCandidates?: number; minConfidence?: number };
}): Promise<FixCandidate> {
  const { signal, locus, invariant, overlay, llm } = args;
  const maxCandidates = args.options?.maxCandidates ?? 3;
  const minConfidence = args.options?.minConfidence ?? 0.5;

  // 1. LLM proposes up to maxCandidates patches.
  const prompt = buildFixPrompt(signal, locus, invariant, maxCandidates);
  const parsed = await requestStructuredJson<unknown>({
    prompt,
    llm,
    stage: "C3-candidateGen",
    model: getModelTier("C3-candidateGen"),
  });
  const candidatePatches = parseProposedFixes(parsed);

  if (candidatePatches.length === 0) {
    throw new Error(
      `generateFixCandidate: LLM returned zero parseable candidates for invariant '${invariant.description}'.`,
    );
  }

  // 2. Filter by minConfidence and rank descending.
  const ranked = candidatePatches
    .filter((c) => c.confidence >= minConfidence)
    .sort((a, b) => b.confidence - a.confidence);

  if (ranked.length === 0) {
    throw new Error(
      `generateFixCandidate: all ${candidatePatches.length} candidate(s) are below minConfidence=${minConfidence}. ` +
      `Highest was ${Math.max(...candidatePatches.map((c) => c.confidence))}.`,
    );
  }

  // 3. For each candidate in ranked order, verify via oracle #2.
  for (const proposed of ranked) {
    const result = await verifyCandidate(proposed, overlay, invariant);
    if (result.invariantHoldsUnderOverlay) {
      return {
        patch: proposed.patch,
        llmRationale: proposed.rationale,
        llmConfidence: proposed.confidence,
        invariantHoldsUnderOverlay: true,
        overlayZ3Verdict: result.z3Verdict,
        audit: result.audit,
      };
    }
    // Candidate failed oracle #2. Try next.
  }

  throw new Error(
    `generateFixCandidate: no candidate survived oracle #2 (tried ${ranked.length}). ` +
    `Invariant '${invariant.description}' could not be satisfied by any LLM proposal.`,
  );
}
