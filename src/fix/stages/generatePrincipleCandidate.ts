// C6: Principle candidate generation.
// May return a plain principle, a principle_with_capability (substrate-extension path), or null.
import type { BugSignal, InvariantClaim, FixCandidate, PrincipleCandidate, LLMProvider, OverlayHandle } from "../types.js";
import type { Db } from "../../db/index.js";
import { tryExistingCapabilities, proposeWithCapability } from "../principleGen.js";

export async function generatePrincipleCandidate(args: {
  signal: BugSignal;
  invariant: InvariantClaim;
  fixCandidate: FixCandidate;
  db: Db;
  llm: LLMProvider;
  overlay?: OverlayHandle;
}): Promise<PrincipleCandidate | null> {
  // 1. If invariant came from an existing principle → no learning needed.
  if (args.invariant.principleId !== null) return null;

  // 2. Try existing capabilities first.
  const attempt = await tryExistingCapabilities(args);
  if (attempt.kind === "ok") return attempt.principle;
  if (attempt.kind === "non_codifiable") return null;

  // 3. Capability gap → propose new capability.
  return await proposeWithCapability({ ...args, gap: attempt.gap, overlay: args.overlay });
}
