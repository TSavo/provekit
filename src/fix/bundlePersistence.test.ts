import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../db/index.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";
import {
  persistBundle,
  loadBundle,
  recordLlmCall,
  enqueuePendingFix,
  oraclesPassedFromAudit,
} from "./bundlePersistence.js";
import {
  _clearArtifactKindRegistry,
  listArtifactKinds,
} from "./artifactKindRegistry.js";
import { registerAll } from "./artifactKinds/index.js";
import {
  fixBundles,
  fixBundleArtifacts,
  llmCalls,
  pendingFixes,
} from "../db/schema/fixBundles.js";
import type { FixBundle, AuditEntry } from "./types.js";
import type { Db } from "../db/index.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function openTestDb(): Db {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-bundle-test-"));
  const db = openDb(join(tmpDir, "test.db"));
  migrate(db, { migrationsFolder: "./drizzle" });
  return db;
}

function makeBundle(overrides: Partial<FixBundle> = {}): FixBundle {
  return {
    bundleId: 0,
    bundleType: "fix",
    bugSignal: {
      source: "report",
      rawText: "TypeError: Cannot read property 'x' of undefined",
      summary: "Null deref at line 42",
      failureDescription: "Crash in foo()",
      codeReferences: [],
    },
    plan: {
      signal: {
        source: "report",
        rawText: "TypeError: Cannot read property 'x' of undefined",
        summary: "Null deref at line 42",
        failureDescription: "Crash in foo()",
        codeReferences: [],
      },
      locus: {
        file: "src/foo.ts",
        line: 42,
        confidence: 0.9,
        primaryNode: "node-001",
        containingFunction: "fn-001",
        relatedFunctions: [],
        dataFlowAncestors: [],
        dataFlowDescendants: [],
        dominanceRegion: [],
        postDominanceRegion: [],
      },
      primaryLayer: "code",
      secondaryLayers: [],
      artifacts: [],
      rationale: "test",
    },
    artifacts: {
      primaryFix: null,
      complementary: [],
      test: null,
      principle: null,
      capabilitySpec: null,
    },
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
    confidence: 0.8,
    auditTrail: [],
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("bundlePersistence", () => {
  let db: Db;

  beforeEach(() => {
    _clearArtifactKindRegistry();
    registerAll();
    db = openTestDb();
  });

  afterEach(() => {
    db.$client.close();
  });

  it("persistBundle writes fix_bundles row and artifact rows", () => {
    const bundle = makeBundle({
      artifacts: {
        primaryFix: {
          patch: { fileEdits: [{ file: "src/foo.ts", newContent: "fixed" }], description: "fix" },
          llmRationale: "it works",
          llmConfidence: 0.9,
          invariantHoldsUnderOverlay: true,
          overlayZ3Verdict: "unsat",
          audit: {
            overlayCreated: true,
            patchApplied: true,
            overlayReindexed: true,
            z3RunMs: 100,
            overlayClosed: false,
          },
        },
        complementary: [],
        test: null,
        principle: null,
        capabilitySpec: null,
      },
    });

    const { bundleId, bundleType } = persistBundle(db, bundle);
    expect(bundleId).toBeGreaterThan(0);
    expect(bundleType).toBe("fix");

    const rows = db.select().from(fixBundles).all();
    expect(rows).toHaveLength(1);
    expect(rows[0].signalSource).toBe("report");

    const artifacts = db.select().from(fixBundleArtifacts).where(eq(fixBundleArtifacts.bundleId, bundleId)).all();
    // code_patch isPresent should be true; others (test, principle, capabilitySpec, complementary) are null/empty
    expect(artifacts.length).toBeGreaterThanOrEqual(1);
    const kinds = artifacts.map((a) => a.kind);
    expect(kinds).toContain("code_patch");
  });

  it("loadBundle round-trips the bundle data", () => {
    const bundle = makeBundle();
    const { bundleId } = persistBundle(db, bundle);
    const loaded = loadBundle(db, bundleId);

    expect(loaded).not.toBeNull();
    expect(loaded!.bundleType).toBe("fix");
    expect(loaded!.bugSignal.source).toBe("report");
    expect(loaded!.bugSignal.summary).toBe("Null deref at line 42");
    expect(loaded!.plan.primaryLayer).toBe("code");
    expect(loaded!.confidence).toBe(0.8);
  });

  it("loadBundle returns null for unknown bundleId", () => {
    const result = loadBundle(db, 99999);
    expect(result).toBeNull();
  });

  it("recordLlmCall writes row with bundle FK", () => {
    const bundle = makeBundle();
    const { bundleId } = persistBundle(db, bundle);

    recordLlmCall(db, bundleId, {
      stage: "C1",
      modelTier: "sonnet",
      prompt: "formulate invariant",
      response: "(assert ...)",
      ms: 250,
    });

    const calls = db.select().from(llmCalls).where(eq(llmCalls.bundleId, bundleId)).all();
    expect(calls).toHaveLength(1);
    expect(calls[0].stage).toBe("C1");
    expect(calls[0].modelTier).toBe("sonnet");
    expect(calls[0].ms).toBe(250);
  });

  it("enqueuePendingFix inserts row with priority", () => {
    const bundle = makeBundle();
    const { bundleId } = persistBundle(db, bundle);

    enqueuePendingFix(db, {
      sourceBundleId: bundleId,
      siteNodeId: "node-042",
      siteFile: "src/bar.ts",
      siteLine: 10,
      reason: "adjacent null deref",
      priority: 5,
    });

    const fixes = db.select().from(pendingFixes).where(eq(pendingFixes.sourceBundleId, bundleId)).all();
    expect(fixes).toHaveLength(1);
    expect(fixes[0].priority).toBe(5);
    expect(fixes[0].siteNodeId).toBe("node-042");
  });

  it("oraclesPassedFromAudit maps C1→[1], C3→[2], C5→[9]", () => {
    const trail: AuditEntry[] = [
      { stage: "C1", kind: "start", detail: "", timestamp: 1 },
      { stage: "C1", kind: "complete", detail: "", timestamp: 2 },
      { stage: "C3", kind: "start", detail: "", timestamp: 3 },
      { stage: "C3", kind: "complete", detail: "", timestamp: 4 },
      { stage: "C5", kind: "start", detail: "", timestamp: 5 },
      { stage: "C5", kind: "complete", detail: "", timestamp: 6 },
    ];
    const oracles = oraclesPassedFromAudit(trail);
    expect(oracles.has(1)).toBe(true);
    expect(oracles.has(2)).toBe(true);
    expect(oracles.has(9)).toBe(true);
    expect(oracles.has(3)).toBe(false);
  });

  it("oraclesPassedFromAudit skips errored stages", () => {
    const trail: AuditEntry[] = [
      { stage: "C1", kind: "start", detail: "", timestamp: 1 },
      { stage: "C1", kind: "complete", detail: "", timestamp: 2 },
      { stage: "C3", kind: "start", detail: "", timestamp: 3 },
      { stage: "C3", kind: "error", detail: "failed", timestamp: 4 },
    ];
    const oracles = oraclesPassedFromAudit(trail);
    expect(oracles.has(1)).toBe(true);
    // C3 errored — oracle 2 should not be included
    expect(oracles.has(2)).toBe(false);
  });

  it("substrate bundle persistence includes capability_spec artifact", () => {
    const capSpec = {
      capabilityName: "null_check",
      schemaTs: "...",
      migrationSql: "...",
      extractorTs: "...",
      extractorTestsTs: "...",
      registryRegistration: "...",
      positiveFixtures: [],
      negativeFixtures: [],
      rationale: "needed for substrate",
    };

    const bundle = makeBundle({
      bundleType: "substrate",
      artifacts: {
        primaryFix: null,
        complementary: [],
        test: null,
        principle: null,
        capabilitySpec: capSpec,
      },
    });

    const { bundleId } = persistBundle(db, bundle);
    const artifacts = db.select().from(fixBundleArtifacts).where(eq(fixBundleArtifacts.bundleId, bundleId)).all();
    const kinds = artifacts.map((a) => a.kind);
    expect(kinds).toContain("capability_spec");
  });

  it("deleting fix_bundles row cascades to artifacts, llm_calls, pending_fixes", () => {
    const bundle = makeBundle();
    const { bundleId } = persistBundle(db, bundle);

    recordLlmCall(db, bundleId, {
      stage: "C1",
      modelTier: "haiku",
      prompt: "p",
      response: "r",
      ms: 10,
    });

    enqueuePendingFix(db, {
      sourceBundleId: bundleId,
      siteNodeId: "n1",
      siteFile: "f.ts",
      siteLine: 1,
      reason: "r",
      priority: 1,
    });

    // Delete the bundle row.
    db.delete(fixBundles).where(eq(fixBundles.id, bundleId)).run();

    // All child rows should be gone.
    expect(db.select().from(fixBundleArtifacts).where(eq(fixBundleArtifacts.bundleId, bundleId)).all()).toHaveLength(0);
    expect(db.select().from(llmCalls).where(eq(llmCalls.bundleId, bundleId)).all()).toHaveLength(0);
    expect(db.select().from(pendingFixes).where(eq(pendingFixes.sourceBundleId, bundleId)).all()).toHaveLength(0);
  });
});
