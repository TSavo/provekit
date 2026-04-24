// C1 stub — landing zone for the formulateInvariant implementation.
import type { BugSignal, BugLocus, InvariantClaim, LLMProvider } from "../types.js";
import { NotImplementedError } from "../types.js";
import type { Db } from "../../db/index.js";

export async function formulateInvariant(args: {
  signal: BugSignal;
  locus: BugLocus;
  db: Db;
  llm: LLMProvider;
}): Promise<InvariantClaim> {
  void args;
  throw new NotImplementedError(
    "C1",
    "formulateInvariant (C1) not yet implemented — B5 orchestrator will route around it when C1 lands",
  );
}
