// D1 stub — landing zone for bundle assembly.
import type {
  BugSignal,
  RemediationPlan,
  BugLocus,
  FixCandidate,
  ComplementaryChange,
  TestArtifact,
  PrincipleCandidate,
  OverlayHandle,
  FixBundle,
} from "../types.js";
import { NotImplementedError } from "../types.js";
import type { Db } from "../../db/index.js";

export async function assembleBundle(args: {
  signal: BugSignal;
  plan: RemediationPlan;
  locus: BugLocus;
  fix: FixCandidate;
  complementary: ComplementaryChange[];
  test: TestArtifact;
  principle: PrincipleCandidate;
  overlay: OverlayHandle;
  db: Db;
}): Promise<FixBundle> {
  void args;
  throw new NotImplementedError(
    "D1",
    "assembleBundle (D1) not yet implemented — B5 orchestrator will route around it when D1 lands",
  );
}
