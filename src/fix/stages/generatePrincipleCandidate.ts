// C6: Principle candidate generation.
//
// Pitch-leak 3 layer 1: returns ARRAY of PrincipleCandidates (length 0..3).
// Index 0 is the canonical shape; remaining entries are alternative AST shapes
// of the same bug class (all share `bugClassId`). Empty array means no
// principle was generated (existing-principle-match, non-codifiable, or
// substrate failure).
import type { BugSignal, InvariantClaim, FixCandidate, PrincipleCandidate, LLMProvider, OverlayHandle } from "../types.js";
import type { Db } from "../../db/index.js";
import type { FixLoopLogger } from "../logger.js";
import { tryExistingCapabilities, proposeWithCapability } from "../principleGen.js";

export async function generatePrincipleCandidate(args: {
  signal: BugSignal;
  invariant: InvariantClaim;
  fixCandidate: FixCandidate;
  db: Db;
  llm: LLMProvider;
  overlay?: OverlayHandle;
  logger?: FixLoopLogger;
}): Promise<PrincipleCandidate[]> {
  // 1. If invariant came from an existing principle → no learning needed.
  if (args.invariant.principleId !== null) return [];

  // 2. Try existing capabilities first.
  const attempt = await tryExistingCapabilities(args);
  if (attempt.kind === "ok") return attempt.principles;
  if (attempt.kind === "non_codifiable") return [];

  // 3. Capability gap → propose new capability (single principle, no alts).
  const substrate = await proposeWithCapability({
    ...args,
    gap: attempt.gap,
    overlay: args.overlay,
  });
  return substrate ? [substrate] : [];
}
