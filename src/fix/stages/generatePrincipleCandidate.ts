// C6: Principle candidate generation.
//
// Pitch-leak 3 layer 1: returns ARRAY of PrincipleCandidates (length 0..3).
// Index 0 is the canonical shape; remaining entries are alternative AST shapes
// of the same bug class (all share `bugClassId`). Empty array means no
// principle was generated (existing-principle-match, non-codifiable, or
// substrate failure).
//
// B3 mechanical-mode (C6m): when args.recognized?.matched === true, the
// principle already exists in the library — C6 produces no new candidate but
// records the customer-fix provenance via appendLibraryProvenance(). The
// principle-array stays empty (consistent with the existing
// principleId !== null branch).
import type { BugSignal, InvariantClaim, FixCandidate, PrincipleCandidate, LLMProvider, OverlayHandle } from "../types.js";
import type { Db } from "../../db/index.js";
import type { FixLoopLogger } from "../logger.js";
import { tryExistingCapabilities, proposeWithCapability } from "../principleGen.js";
import type { RecognizeResult } from "./recognize.js";
import { appendLibraryProvenance } from "./recognizeProvenance.js";

export async function generatePrincipleCandidate(args: {
  signal: BugSignal;
  invariant: InvariantClaim;
  fixCandidate: FixCandidate;
  db: Db;
  llm: LLMProvider;
  overlay?: OverlayHandle;
  logger?: FixLoopLogger;
  /** B3 mechanical-mode input. When matched, C6m runs (provenance append, no LLM). */
  recognized?: RecognizeResult;
  /**
   * Host project root, optional. Threaded into tryExistingCapabilities so
   * the C6 principle prompt fragment resolves via better-prompts.
   */
  projectRoot?: string;
}): Promise<PrincipleCandidate[]> {
  // C6m: B3 recognized path — append provenance to the existing library entry.
  if (args.recognized && args.recognized.matched) {
    appendLibraryProvenance({
      principleId: args.recognized.principleId,
      entry: {
        source: "customer-fix",
        timestamp: new Date().toISOString(),
        bugId: args.signal.codeReferences[0]
          ? `${args.signal.codeReferences[0].file}:${args.signal.codeReferences[0].line ?? 0}`
          : undefined,
      },
      logger: args.logger,
    });
    return [];
  }

  // 1. If invariant came from an existing principle → no learning needed.
  if (args.invariant.principleId !== null) return [];

  // 2. Try existing capabilities first.
  const attempt = await tryExistingCapabilities(args);
  if (attempt.kind === "ok") return attempt.principles;
  if (attempt.kind === "non_codifiable") return [];

  // 3. Capability gap → propose new capability (single principle, no alts).
  // Two routes get here:
  //   (a) compile-time capability_gap — LLM said "needs_capability" or its
  //       canonical shape referenced an unknown column. Standard substrate
  //       path with the LLM-named gap.
  //   (b) all_shapes_rejected — LLM emitted bare-principle shapes that all
  //       compiled but adversarial validation rejected as too-broad. Build
  //       a predicate-shaped gap from the invariant description so the
  //       capability agent has a clean target to model. Including the raw
  //       adversarial metrics ("false-positive pass: 3/3") as the gap
  //       confused the agent (v15: 11 min of exploration with no spec
  //       written) — agents need a predicate description, not validation
  //       metrics.
  const gap =
    attempt.kind === "capability_gap"
      ? attempt.gap
      : `Express the predicate: ${args.invariant.description}\n\n` +
        `Stock SAST capabilities cannot narrow this precisely — bare-principle ` +
        `shapes (${attempt.rejectedShapes.map((s) => s.name).join(", ")}) ` +
        `compiled but matched too broadly. The new capability must encode the ` +
        `contextual semantic that distinguishes the bug shape from look-alike ` +
        `non-bug code.`;

  const substrate = await proposeWithCapability({
    ...args,
    gap,
    overlay: args.overlay,
  });
  return substrate ? [substrate] : [];
}
