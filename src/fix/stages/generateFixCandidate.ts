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
import { buildFixPrompt, parseProposedFixes, verifyCandidate } from "../candidateGen.js";

export async function generateFixCandidate(args: {
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
