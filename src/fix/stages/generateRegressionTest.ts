// C5 stub — landing zone for regression test generation.
import type { FixCandidate, BugSignal, BugLocus, OverlayHandle, TestArtifact, LLMProvider } from "../types.js";
import { NotImplementedError } from "../types.js";

export async function generateRegressionTest(args: {
  fix: FixCandidate;
  signal: BugSignal;
  locus: BugLocus;
  overlay: OverlayHandle;
  llm: LLMProvider;
}): Promise<TestArtifact> {
  void args;
  throw new NotImplementedError(
    "C5",
    "generateRegressionTest (C5) not yet implemented — B5 orchestrator will route around it when C5 lands",
  );
}
