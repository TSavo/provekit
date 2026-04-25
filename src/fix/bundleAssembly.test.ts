/**
 * D1b: Tests for assembleBundle + BundleCoherenceFailed.
 *
 * Uses in-memory SQLite + registered artifact kinds.
 * All oracle calls use stub runners (no real Z3/vitest invocations).
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mkdtempSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../db/index.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import type { Db } from "../db/index.js";
import {
  _clearArtifactKindRegistry,
} from "./artifactKindRegistry.js";
import { registerAll } from "./artifactKinds/index.js";
import type {
  BugSignal,
  RemediationPlan,
  BugLocus,
  FixCandidate,
  ComplementaryChange,
  TestArtifact,
  PrincipleCandidate,
  OverlayHandle,
  CapabilitySpec,
  InvariantClaim,
} from "./types.js";
import { assembleBundle, BundleCoherenceFailed } from "./bundleAssembly.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function openTestDb(): Db {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-assembly-test-"));
  const db = openDb(join(tmpDir, "test.db"));
  migrate(db, { migrationsFolder: "./drizzle" });
  return db;
}

function makeSastDb(): Db {
  const sastDb = openDb(":memory:");
  // Create a minimal nodes table so oracle #11 passes
  sastDb.$client.exec("CREATE TABLE nodes (id TEXT PRIMARY KEY)");
  sastDb.$client.exec("CREATE TABLE node_children (parent_id TEXT, child_id TEXT)");
  sastDb.$client.exec("INSERT INTO nodes VALUES ('node-1')");
  return sastDb;
}

function makeOverlay(sastDb: Db): OverlayHandle {
  return {
    worktreePath: "/tmp/fake-overlay",
    sastDbPath: "/tmp/fake.db",
    sastDb,
    baseRef: "HEAD",
    modifiedFiles: new Set(),
    closed: false,
  };
}

const SIGNAL: BugSignal = {
  source: "report",
  rawText: "TypeError: Cannot read property 'x' of undefined",
  summary: "Null deref at line 42",
  failureDescription: "Crash in foo()",
  codeReferences: [],
};

const LOCUS: BugLocus = {
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
};

const PLAN: RemediationPlan = {
  signal: SIGNAL,
  locus: LOCUS,
  primaryLayer: "code",
  secondaryLayers: [],
  artifacts: [],
  rationale: "test plan",
};

function makeFix(): FixCandidate {
  return {
    patch: {
      fileEdits: [{ file: "src/foo.ts", newContent: "export const x = 1;" }],
      description: "guard added",
    },
    llmRationale: "fixed by adding null guard",
    llmConfidence: 0.85,
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "unsat",
    audit: {
      overlayCreated: true,
      patchApplied: true,
      overlayReindexed: true,
      z3RunMs: 50,
      overlayClosed: false,
    },
  };
}

function makeTest(): TestArtifact {
  return {
    testFilePath: "src/foo.regression.test.ts",
    testName: "regression: null deref at line 42",
    testCode: "import { it } from 'vitest'; it('test', () => {});",
    witnessInputs: { x: null },
    passesOnFixedCode: true,
    failsOnOriginalCode: true,
    audit: {
      fixedRunStdout: "PASS",
      fixedRunExitCode: 0,
      originalRunStdout: "FAIL",
      originalRunExitCode: 1,
      mutationApplied: true,
      mutationReverted: true,
    },
  };
}

function makeCapabilitySpec(): CapabilitySpec {
  return {
    capabilityName: "null_guard",
    schemaTs: "// schema",
    migrationSql: "-- migration",
    extractorTs: "// extractor",
    extractorTestsTs: "// tests",
    registryRegistration: "// registration",
    positiveFixtures: [],
    negativeFixtures: [],
    rationale: "detects null deref patterns",
  };
}

function makePrincipleWithCapability(): PrincipleCandidate {
  return {
    kind: "principle_with_capability",
    name: "null_guard_principle",
    dslSource: "MATCH ...",
    smtTemplate: "(assert ...)",
    teachingExample: { domain: "null", explanation: "example", smt2: "(check-sat)" },
    adversarialValidation: [],
    latentSiteMatches: [],
    capabilitySpec: makeCapabilitySpec(),
  };
}

function makePrinciple(): PrincipleCandidate {
  return {
    kind: "principle",
    name: "null_guard_principle",
    dslSource: "MATCH ...",
    smtTemplate: "(assert ...)",
    teachingExample: { domain: "null", explanation: "example", smt2: "(check-sat)" },
    adversarialValidation: [],
    latentSiteMatches: [],
  };
}

/** A vitest runner stub that always passes */
const passingRunner = () => ({ exitCode: 0, stdout: "all tests passed", stderr: "" });
/** A vitest runner stub that always fails */
const failingRunner = () => ({ exitCode: 1, stdout: "", stderr: "test failure" });

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("assembleBundle", () => {
  let db: Db;
  let sastDb: Db;

  beforeEach(() => {
    _clearArtifactKindRegistry();
    registerAll();
    db = openTestDb();
    sastDb = makeSastDb();
  });

  afterEach(() => {
    db.$client.close();
    sastDb.$client.close();
  });

  // Test 1: fix bundle happy path
  it("fix bundle happy path: all artifacts present, all oracles pass → returns FixBundle with bundleId", async () => {
    const overlay = makeOverlay(sastDb);
    const fix = makeFix();
    const test = makeTest();

    const bundle = await assembleBundle({
      signal: SIGNAL,
      plan: PLAN,
      locus: LOCUS,
      fix,
      complementary: [],
      test,
      principle: null,
      overlay,
      db,
      vitestRunner: passingRunner,
    });

    expect(bundle.bundleId).toBeGreaterThan(0);
    expect(bundle.bundleType).toBe("fix");
    expect(bundle.artifacts.primaryFix).toBe(fix);
    expect(bundle.artifacts.test).toBe(test);
    expect(bundle.confidence).toBeGreaterThan(0);
  });

  // Test 2: substrate bundle — oracle #15 runs (MVP stub passes)
  it("substrate bundle: principle_with_capability → bundleType=substrate, oracle #15 pass", async () => {
    const overlay = makeOverlay(sastDb);
    const fix = makeFix();
    const principle = makePrincipleWithCapability();

    const bundle = await assembleBundle({
      signal: SIGNAL,
      plan: PLAN,
      locus: LOCUS,
      fix,
      complementary: [],
      test: null,
      principle,
      overlay,
      db,
      vitestRunner: passingRunner,
    });

    expect(bundle.bundleType).toBe("substrate");
    expect(bundle.artifacts.capabilitySpec).toBeDefined();
    expect(bundle.bundleId).toBeGreaterThan(0);
  });

  // Test 3: audit trail short-circuit — already-fired oracles NOT re-run
  it("audit-trail short-circuit: oracles already-fired skip re-execution", async () => {
    const overlay = makeOverlay(sastDb);
    const fix = makeFix();

    // Inject an audit trail that covers C1 (oracle #1) and C3 (oracle #2)
    const existingAuditTrail = [
      { stage: "C1", kind: "complete" as const, detail: "C1 done", timestamp: Date.now() },
      { stage: "C3", kind: "complete" as const, detail: "C3 done", timestamp: Date.now() },
    ];

    // The vitestRunner should be called only if oracle #10 is new.
    // C5 not in the existing trail, so oracle #9 is not already-fired.
    // But oracle #10 is NEW — verify it calls runner exactly once (or twice on retry).
    let runnerCallCount = 0;
    const countingRunner = () => {
      runnerCallCount++;
      return { exitCode: 0, stdout: "pass", stderr: "" };
    };

    await assembleBundle({
      signal: SIGNAL,
      plan: PLAN,
      locus: LOCUS,
      fix,
      complementary: [],
      test: null,
      principle: null,
      overlay,
      db,
      existingAuditTrail,
      vitestRunner: countingRunner,
    });

    // Oracle #10 (full suite) is NEW and should run once
    expect(runnerCallCount).toBe(1);
  });

  // Test 4: oracle failure throws BundleCoherenceFailed
  it("oracle failure throws BundleCoherenceFailed", async () => {
    const overlay = makeOverlay(sastDb);
    const fix = makeFix();

    // oracle #10 fails
    await expect(
      assembleBundle({
        signal: SIGNAL,
        plan: PLAN,
        locus: LOCUS,
        fix,
        complementary: [],
        test: null,
        principle: null,
        overlay,
        db,
        vitestRunner: failingRunner,
      }),
    ).rejects.toThrow(BundleCoherenceFailed);
  });

  // Test 5: confidence dampening for > 5 artifacts
  it("confidence dampened for 6-artifact bundle (fix + 5 complementary)", async () => {
    const overlay = makeOverlay(sastDb);
    const fix = makeFix();
    fix.llmConfidence = 1.0;

    const complementary: ComplementaryChange[] = Array.from({ length: 5 }, (_, i) => ({
      kind: "adjacent_site_fix" as const,
      targetNodeId: `node-${i}`,
      patch: { fileEdits: [], description: "comp" },
      rationale: "comp",
      verifiedAgainstOverlay: true,
      overlayZ3Verdict: "unsat" as const,
      priority: i,
      audit: {
        siteKind: "adjacent_site_fix" as const,
        discoveredVia: "llm_reflection" as const,
        z3RunMs: 10,
      },
    }));

    const bundle = await assembleBundle({
      signal: SIGNAL,
      plan: PLAN,
      locus: LOCUS,
      fix,
      complementary,
      test: null,
      principle: null,
      overlay,
      db,
      vitestRunner: passingRunner,
    });

    // fix.llmConfidence = 1.0; artifact count = 1 (fix) + 5 (comp) = 6 > 5 → 10% dampening
    expect(bundle.confidence).toBeLessThan(fix.llmConfidence);
    expect(bundle.confidence).toBeCloseTo(0.9, 5);
  });

  // Test 6: persistBundle called — bundleId is set
  it("persistBundle is called and bundleId is assigned on the returned bundle", async () => {
    const overlay = makeOverlay(sastDb);
    const fix = makeFix();

    const bundle = await assembleBundle({
      signal: SIGNAL,
      plan: PLAN,
      locus: LOCUS,
      fix,
      complementary: [],
      test: null,
      principle: null,
      overlay,
      db,
      vitestRunner: passingRunner,
    });

    expect(bundle.bundleId).toBeGreaterThan(0);
    // Verify DB actually has the row
    const { fixBundles } = await import("../db/schema/fixBundles.js");
    const rows = db.select().from(fixBundles).all();
    expect(rows.length).toBe(1);
    expect(rows[0]!.id).toBe(bundle.bundleId);
  });

  // Test 7: all coherence flags true when all oracles pass
  it("all required oracles pass → bundle coherence flags all true", async () => {
    const overlay = makeOverlay(sastDb);
    const fix = makeFix();
    const test = makeTest();

    // Supply a C6 audit trail entry so substrate-style oracles are considered already-fired.
    // For a fix bundle (no principle), C6-fired oracles (#6,#14,#16,#17,#18) are N/A.
    const bundle = await assembleBundle({
      signal: SIGNAL,
      plan: PLAN,
      locus: LOCUS,
      fix,
      complementary: [],
      test,
      principle: null,
      overlay,
      db,
      vitestRunner: passingRunner,
    });

    expect(bundle.coherence.sastStructural).toBe(true);
    expect(bundle.coherence.fullSuiteGreen).toBe(true);
    expect(bundle.coherence.noNewGapsIntroduced).toBe(true);
    // z3SemanticConsistency: oracle #5 is trivially true with no invariants
    expect(bundle.coherence.z3SemanticConsistency).toBe(true);
    // Substrate flags are null for a fix bundle
    expect(bundle.coherence.migrationSafe).toBeNull();
    expect(bundle.coherence.extractorCoverage).toBeNull();
    expect(bundle.coherence.substrateConsistency).toBeNull();
  });

  // Test 8: oracle #11 (SAST coherence) fails → throws BundleCoherenceFailed naming oracle #11
  it("oracle #11 (SAST structural) fails → throws BundleCoherenceFailed", async () => {
    // Create an empty SAST DB so oracle #11 sees 0 nodes → fails
    const emptySastDb = openDb(":memory:");
    emptySastDb.$client.exec("CREATE TABLE nodes (id TEXT PRIMARY KEY)");
    emptySastDb.$client.exec("CREATE TABLE node_children (parent_id TEXT, child_id TEXT)");
    // Insert ZERO rows — oracle #11 requires count > 0
    const overlay = makeOverlay(emptySastDb);
    const fix = makeFix();

    await expect(
      assembleBundle({
        signal: SIGNAL,
        plan: PLAN,
        locus: LOCUS,
        fix,
        complementary: [],
        test: null,
        principle: null,
        overlay,
        db,
        vitestRunner: passingRunner,
      }),
    ).rejects.toThrow(BundleCoherenceFailed);

    await expect(
      assembleBundle({
        signal: SIGNAL,
        plan: PLAN,
        locus: LOCUS,
        fix,
        complementary: [],
        test: null,
        principle: null,
        overlay,
        db,
        vitestRunner: passingRunner,
      }),
    ).rejects.toMatchObject({ oracleId: 11 });

    emptySastDb.$client.close();
  });

  // Test 9: oracle #10 (full suite) fails → throws BundleCoherenceFailed
  it("oracle #10 (full suite) fails → throws BundleCoherenceFailed", async () => {
    const overlay = makeOverlay(sastDb);
    const fix = makeFix();

    await expect(
      assembleBundle({
        signal: SIGNAL,
        plan: PLAN,
        locus: LOCUS,
        fix,
        complementary: [],
        test: null,
        principle: null,
        overlay,
        db,
        vitestRunner: failingRunner,
      }),
    ).rejects.toThrow(BundleCoherenceFailed);

    await expect(
      assembleBundle({
        signal: SIGNAL,
        plan: PLAN,
        locus: LOCUS,
        fix,
        complementary: [],
        test: null,
        principle: null,
        overlay,
        db,
        vitestRunner: failingRunner,
      }),
    ).rejects.toMatchObject({ oracleId: 10 });
  });

  // Test 10: substrate bundle, oracle #14 not confirmed via C6 → throws BundleCoherenceFailed
  it("substrate bundle: oracle #14 not confirmed (C6 errored) → throws BundleCoherenceFailed", async () => {
    const overlay = makeOverlay(sastDb);
    const fix = makeFix();
    const principle = makePrincipleWithCapability();

    // Inject a C6 ERROR entry in the existing audit trail. The reconstructed trail
    // will also have a C6 complete (because principle !== null), but the error entry
    // causes oraclesPassedFromAudit to skip C6 → oracle #14 not in alreadyFired.
    const existingAuditTrail = [
      { stage: "C6", kind: "error" as const, detail: "C6 failed — runOracle14 rejected DROP TABLE", timestamp: Date.now() },
    ];

    await expect(
      assembleBundle({
        signal: SIGNAL,
        plan: PLAN,
        locus: LOCUS,
        fix,
        complementary: [],
        test: null,
        principle,
        overlay,
        db,
        existingAuditTrail,
        vitestRunner: passingRunner,
      }),
    ).rejects.toThrow(BundleCoherenceFailed);

    await expect(
      assembleBundle({
        signal: SIGNAL,
        plan: PLAN,
        locus: LOCUS,
        fix,
        complementary: [],
        test: null,
        principle,
        overlay,
        db,
        existingAuditTrail,
        vitestRunner: passingRunner,
      }),
    ).rejects.toMatchObject({ oracleId: 14 });
  });

  // Test 11: oracle runners are actually invoked (smoke check against empty-registry regression)
  it("oracle #10 runner is invoked at least once — guards against empty-registry no-op path", async () => {
    const overlay = makeOverlay(sastDb);
    const fix = makeFix();

    let runnerInvokeCount = 0;
    const countingPassingRunner = () => {
      runnerInvokeCount++;
      return { exitCode: 0, stdout: "all tests passed", stderr: "" };
    };

    await assembleBundle({
      signal: SIGNAL,
      plan: PLAN,
      locus: LOCUS,
      fix,
      complementary: [],
      test: null,
      principle: null,
      overlay,
      db,
      vitestRunner: countingPassingRunner,
    });

    // If the artifact kind registry was empty, newOracles would be empty, and
    // the vitestRunner (oracle #10) would never be called. Asserting > 0 proves
    // that the registry was populated and oracles actually ran.
    expect(runnerInvokeCount).toBeGreaterThan(0);
  });
});
