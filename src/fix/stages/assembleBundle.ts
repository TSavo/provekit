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
  AuditEntry,
} from "../types.js";
import type { Db } from "../../db/index.js";
import type { FixLoopLogger } from "../logger.js";
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
  /**
   * Pitch-leak 3 layer 1: alternative AST shapes for the same bug class.
   * Stored alongside the canonical `principle` so the principle library
   * captures multi-shape coverage. Defaults to [] when not supplied.
   */
  alternateShapes?: PrincipleCandidate[];
  overlay: OverlayHandle;
  db: Db;
  /** Optional pre-seeded audit trail from the orchestrator. */
  existingAuditTrail?: AuditEntry[];
  /** Optional oracle #10 runner injection (for tests). */
  vitestRunner?: (overlay: OverlayHandle) => { exitCode: number; stdout: string; stderr: string };
  logger?: FixLoopLogger;
}): Promise<FixBundle> {
  return _assembleBundle(args);
}
