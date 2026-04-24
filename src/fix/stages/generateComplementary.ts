// C4 stub — landing zone for complementary change discovery.
import type { FixCandidate, BugLocus, OverlayHandle, ComplementaryChange, LLMProvider } from "../types.js";
import { NotImplementedError } from "../types.js";
import type { Db } from "../../db/index.js";

export async function generateComplementary(args: {
  fix: FixCandidate;
  locus: BugLocus;
  overlay: OverlayHandle;
  db: Db;
  llm: LLMProvider;
  maxSites: number;
}): Promise<ComplementaryChange[]> {
  void args;
  throw new NotImplementedError(
    "C4",
    "generateComplementary (C4) not yet implemented — B5 orchestrator will route around it when C4 lands",
  );
}
