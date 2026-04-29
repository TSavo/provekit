/**
 * Bundle stage — bug-fix workflow's terminal artifact assembly.
 *
 * Wraps assembleBundle() in a Stage<I, O>. Produces the FixBundle:
 * the complete deployable package containing the patch, test,
 * principle, complementary changes, AND the Oracle #10 coherence
 * verdicts (sastStructural, z3SemanticConsistency, fullSuiteGreen,
 * etc.).
 *
 * Cache contract — same shape as do-the-work:
 *   The memento IS the unit of work. Bundle unit of work = "the
 *   complete artifact package, with all upstream verdicts AND
 *   bundle-level coherence verdicts attached." Caching less than
 *   that produces a fragment that needs re-verification.
 *   On cache hit, the FixBundle reconstructs with every coherence
 *   field intact; downstream consumers trust those without re-running
 *   Oracle #10.
 *
 * Side effect: the underlying assembleBundle persists the bundle to
 * the DB (assigns bundleId, writes audit rows). On cache hit, the
 * persistence is NOT re-done — consumers should use the in-memory
 * struct rather than re-querying by bundleId. The persistence-
 * separation refactor (split "produce bundle struct" from "persist
 * bundle row") is a follow-up that lets cache hits also re-persist
 * idempotently.
 *
 * Input hashing: signal, plan, locus, fix, test, principle,
 * complementary, alternateShapes, invariants, triggeringGapId,
 * overlay.baseRef. Excludes runtime fields and test injection
 * points (vitestRunner) for the same reasons as do-the-work.
 */

import { assembleBundle } from "../../fix/stages/assembleBundle.js";
import type {
  BugLocus,
  ComplementaryChange,
  FixBundle,
  FixCandidate,
  IntentSignal,
  InvariantClaim,
  OverlayHandle,
  PrincipleCandidate,
  RemediationPlan,
  TestArtifact,
  AuditEntry,
} from "../../fix/types.js";
import type { Db } from "../../db/index.js";
import type { FixLoopLogger } from "../../fix/logger.js";
import type { Stage } from "../types.js";

export const BUNDLE_CAPABILITY = "bundle";

export interface BundleStageInput {
  signal: IntentSignal;
  plan: RemediationPlan;
  locus: BugLocus;
  fix: FixCandidate;
  complementary: ComplementaryChange[];
  test: TestArtifact | null;
  principle: PrincipleCandidate | null;
  alternateShapes?: PrincipleCandidate[];
  overlay: OverlayHandle;
  /** Audit trail carried forward from upstream stages. */
  existingAuditTrail?: AuditEntry[];
  /** Test-only injection. Hash-excluded. */
  vitestRunner?: (overlay: OverlayHandle) => {
    exitCode: number;
    stdout: string;
    stderr: string;
  };
  /** Optional gap-report ID for oracle #13. */
  triggeringGapId?: number;
  /** Optional explicit invariants for oracle #5. */
  invariants?: InvariantClaim[];
}

export interface MakeBundleStageDeps {
  db: Db;
  logger?: FixLoopLogger;
  /** Override producer identity. Default: "bundle@v1". */
  producerVersion?: string;
}

export function makeBundleStage(
  deps: MakeBundleStageDeps,
): Stage<BundleStageInput, FixBundle> {
  const producedBy = deps.producerVersion ?? "bundle@v1";

  return {
    name: "bundle",
    producedBy,

    serializeInput(input) {
      return {
        signal: input.signal,
        plan: input.plan,
        locus: input.locus,
        fix: input.fix,
        complementary: input.complementary,
        test: input.test ?? null,
        principle: input.principle ?? null,
        alternateShapes: input.alternateShapes ?? [],
        existingAuditTrail: input.existingAuditTrail ?? null,
        triggeringGapId: input.triggeringGapId ?? null,
        invariants: input.invariants ?? null,
        overlayBaseRef: input.overlay.baseRef,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as FixBundle;
    },

    async run(input) {
      return assembleBundle({
        signal: input.signal,
        plan: input.plan,
        locus: input.locus,
        fix: input.fix,
        complementary: input.complementary,
        test: input.test,
        principle: input.principle,
        alternateShapes: input.alternateShapes,
        overlay: input.overlay,
        db: deps.db,
        existingAuditTrail: input.existingAuditTrail,
        vitestRunner: input.vitestRunner,
        logger: deps.logger,
      });
    },
  };
}
