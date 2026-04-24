// C3 stub — landing zone for fix candidate generation.
import type { BugSignal, BugLocus, InvariantClaim, OverlayHandle, FixCandidate, LLMProvider } from "../types.js";
import { NotImplementedError } from "../types.js";

export async function generateFixCandidate(args: {
  signal: BugSignal;
  locus: BugLocus;
  invariant: InvariantClaim;
  overlay: OverlayHandle;
  llm: LLMProvider;
}): Promise<FixCandidate> {
  void args;
  throw new NotImplementedError(
    "C3",
    "generateFixCandidate (C3) not yet implemented — B5 orchestrator will route around it when C3 lands",
  );
}
