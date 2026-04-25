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
} from "../types.js";
import type { FixLoopLogger } from "../logger.js";
import { buildFixPrompt, parseProposedFixes, verifyCandidate, buildAgentFixPrompt } from "../candidateGen.js";
import { runAgentInOverlay } from "../captureChange.js";

export async function generateFixCandidate(args: {
  signal: BugSignal;
  locus: BugLocus;
  invariant: InvariantClaim;
  overlay: OverlayHandle;
  llm: LLMProvider;
  options?: { maxCandidates?: number; minConfidence?: number };
  logger?: FixLoopLogger;
}): Promise<FixCandidate> {
  // Agent path: if the LLM provider supports agent(), use capture-the-change.
  if (args.llm.agent) {
    return generateFixCandidateViaAgent({ ...args, logger: args.logger });
  }
  // Legacy JSON-patch path.
  return generateFixCandidateViaJson(args);
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
}): Promise<FixCandidate> {
  const { signal, locus, invariant, overlay } = args;

  const prompt = buildAgentFixPrompt(signal, locus, invariant, overlay);

  // First attempt.
  const { patch, rationale } = await runAgentInOverlay({
    overlay,
    llm: args.llm,
    prompt,
    maxTurns: 20,
    logger: args.logger,
  });

  // verifyCandidate applies the patch to the overlay (idempotent rewrite),
  // re-indexes, and runs oracle #2.
  const proposed = { patch, rationale, confidence: 1.0 };
  const result = await verifyCandidate(proposed, overlay, invariant);

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
    maxTurns: 20,
    logger: args.logger,
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
  const rawResponse = await llm.complete({ prompt });
  const candidatePatches = parseProposedFixes(rawResponse);

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
