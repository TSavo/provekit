// C6 stub — landing zone for principle candidate generation.
// May return a plain principle OR a principle_with_capability (substrate-extension path).
import type { BugSignal, InvariantClaim, FixCandidate, PrincipleCandidate, LLMProvider } from "../types.js";
import { NotImplementedError } from "../types.js";
import type { Db } from "../../db/index.js";

export async function generatePrincipleCandidate(args: {
  signal: BugSignal;
  invariant: InvariantClaim;
  fixCandidate: FixCandidate;
  db: Db;
  llm: LLMProvider;
}): Promise<PrincipleCandidate> {
  void args;
  throw new NotImplementedError(
    "C6",
    "generatePrincipleCandidate (C6) not yet implemented — B5 orchestrator will route around it when C6 lands",
  );
}
