/**
 * D1b: assembleBundle stage — delegates to bundleAssembly.ts.
 *
 * Previously a stub that threw NotImplementedError. Now routes to the real
 * bundle coherence oracle runner.
 */
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
import type { Db } from "../../db/index.js";
import { assembleBundle as _assembleBundle } from "../bundleAssembly.js";

export { BundleCoherenceFailed } from "../bundleAssembly.js";

export async function assembleBundle(args: {
  signal: BugSignal;
  plan: RemediationPlan;
  locus: BugLocus;
  fix: FixCandidate;
  complementary: ComplementaryChange[];
  test: TestArtifact | null;
  principle: PrincipleCandidate | null;
  overlay: OverlayHandle;
  db: Db;
}): Promise<FixBundle> {
  return _assembleBundle(args);
}
