/**
 * D1a: Bundle persistence helpers.
 *
 * Wraps the four new fix-loop tables (fix_bundles, fix_bundle_artifacts,
 * llm_calls, pending_fixes) in typed helper functions.
 */

import { eq } from "drizzle-orm";
import type { Db } from "../db/index.js";
import {
  fixBundles,
  fixBundleArtifacts,
  llmCalls,
  pendingFixes,
} from "../db/schema/fixBundles.js";
import { listArtifactKinds } from "./artifactKindRegistry.js";
import type { FixBundle, AuditEntry } from "./types.js";

// ---------------------------------------------------------------------------
// Stage → oracle ID mapping (used by oraclesPassedFromAudit)
// ---------------------------------------------------------------------------

const STAGE_TO_ORACLES: Record<string, number[]> = {
  C1: [1],
  C3: [2],
  C4: [3],
  C5: [9],
  C6: [6, 14, 16, 17, 18],
};

// ---------------------------------------------------------------------------
// persistBundle
// ---------------------------------------------------------------------------

export function persistBundle(
  db: Db,
  bundle: FixBundle,
): { bundleId: number; bundleType: string } {
  return db.transaction((tx) => {
    const now = Date.now();
    const locus = bundle.plan.locus;

    const inserted = tx
      .insert(fixBundles)
      .values({
        bundleType: bundle.bundleType,
        createdAt: now,
        signalRawtext: bundle.bugSignal.rawText,
        signalSource: bundle.bugSignal.source,
        signalSummary: bundle.bugSignal.summary,
        primaryLayer: bundle.plan.primaryLayer,
        locusFile: locus?.file ?? "",
        locusLine: locus?.line ?? 0,
        locusPrimaryNode: locus?.primaryNode ?? null,
        appliedAt: null,
        commitSha: null,
        confidence: bundle.confidence,
      })
      .returning({ id: fixBundles.id })
      .all();

    const bundleId = inserted[0].id;

    for (const kind of listArtifactKinds()) {
      if (!kind.isPresent(bundle.artifacts)) continue;

      let payload: unknown;
      if (kind.name === "code_patch") {
        payload = bundle.artifacts.primaryFix;
      } else if (kind.name === "regression_test") {
        payload = bundle.artifacts.test;
      } else if (kind.name === "principle_candidate") {
        payload = bundle.artifacts.principle;
      } else if (kind.name === "capability_spec") {
        payload = bundle.artifacts.capabilitySpec;
      } else if (kind.name === "complementary_change") {
        payload = bundle.artifacts.complementary;
      } else {
        payload = null;
      }

      tx.insert(fixBundleArtifacts).values({
        bundleId,
        kind: kind.name,
        payloadJson: JSON.stringify(payload),
        passedOracles: JSON.stringify([]),
        verifiedAt: now,
      }).run();
    }

    return { bundleId, bundleType: bundle.bundleType };
  });
}

// ---------------------------------------------------------------------------
// loadBundle
// ---------------------------------------------------------------------------

export function loadBundle(db: Db, bundleId: number): FixBundle | null {
  const row = db.select().from(fixBundles).where(eq(fixBundles.id, bundleId)).all()[0];
  if (!row) return null;

  const artifactRows = db
    .select()
    .from(fixBundleArtifacts)
    .where(eq(fixBundleArtifacts.bundleId, bundleId))
    .all();

  const artifactMap: Record<string, unknown> = {};
  for (const ar of artifactRows) {
    artifactMap[ar.kind] = JSON.parse(ar.payloadJson);
  }

  const artifacts: FixBundle["artifacts"] = {
    primaryFix: (artifactMap["code_patch"] as FixBundle["artifacts"]["primaryFix"]) ?? null,
    test: (artifactMap["regression_test"] as FixBundle["artifacts"]["test"]) ?? null,
    principle: (artifactMap["principle_candidate"] as FixBundle["artifacts"]["principle"]) ?? null,
    capabilitySpec:
      (artifactMap["capability_spec"] as FixBundle["artifacts"]["capabilitySpec"]) ?? null,
    complementary:
      (artifactMap["complementary_change"] as FixBundle["artifacts"]["complementary"]) ?? [],
  };

  const bundle: FixBundle = {
    bundleId: row.id,
    bundleType: row.bundleType as "fix" | "substrate",
    bugSignal: {
      source: row.signalSource,
      rawText: row.signalRawtext,
      summary: row.signalSummary,
      failureDescription: "",
      codeReferences: [],
    },
    plan: {
      signal: {
        source: row.signalSource,
        rawText: row.signalRawtext,
        summary: row.signalSummary,
        failureDescription: "",
        codeReferences: [],
      },
      locus: row.locusFile
        ? {
            file: row.locusFile,
            line: row.locusLine,
            confidence: 0,
            primaryNode: row.locusPrimaryNode ?? "",
            containingFunction: "",
            relatedFunctions: [],
            dataFlowAncestors: [],
            dataFlowDescendants: [],
            dominanceRegion: [],
            postDominanceRegion: [],
          }
        : null,
      primaryLayer: row.primaryLayer,
      secondaryLayers: [],
      artifacts: [],
      rationale: "",
    },
    artifacts,
    coherence: {
      sastStructural: false,
      z3SemanticConsistency: false,
      fullSuiteGreen: false,
      noNewGapsIntroduced: false,
      migrationSafe: null,
      crossCodebaseRegression: null,
      extractorCoverage: null,
      substrateConsistency: null,
      principleNeedsCapability: null,
    },
    confidence: row.confidence,
    auditTrail: [],
  };

  return bundle;
}

// ---------------------------------------------------------------------------
// recordLlmCall
// ---------------------------------------------------------------------------

export function recordLlmCall(
  db: Db,
  bundleId: number,
  params: {
    stage: string;
    modelTier: string;
    prompt: string;
    response: string;
    seed?: number;
    ms: number;
  },
): void {
  db.insert(llmCalls).values({
    bundleId,
    stage: params.stage,
    modelTier: params.modelTier,
    prompt: params.prompt,
    response: params.response,
    seed: params.seed ?? null,
    ms: params.ms,
    calledAt: Date.now(),
  }).run();
}

// ---------------------------------------------------------------------------
// enqueuePendingFix
// ---------------------------------------------------------------------------

export function enqueuePendingFix(
  db: Db,
  params: {
    sourceBundleId: number;
    siteNodeId: string;
    siteFile: string;
    siteLine: number;
    reason: string;
    priority: number;
  },
): void {
  db.insert(pendingFixes).values({
    sourceBundleId: params.sourceBundleId,
    siteNodeId: params.siteNodeId,
    siteFile: params.siteFile,
    siteLine: params.siteLine,
    reason: params.reason,
    priority: params.priority,
    createdAt: Date.now(),
  }).run();
}

// ---------------------------------------------------------------------------
// oraclesPassedFromAudit
// ---------------------------------------------------------------------------

export function oraclesPassedFromAudit(trail: AuditEntry[]): Set<number> {
  const result = new Set<number>();

  // Collect stages that completed without error.
  const completedStages = new Set<string>();
  const erroredStages = new Set<string>();

  for (const entry of trail) {
    if (entry.kind === "complete") completedStages.add(entry.stage);
    if (entry.kind === "error") erroredStages.add(entry.stage);
  }

  for (const stage of completedStages) {
    if (erroredStages.has(stage)) continue;
    const oracles = STAGE_TO_ORACLES[stage];
    if (oracles) {
      for (const o of oracles) result.add(o);
    }
  }

  return result;
}
