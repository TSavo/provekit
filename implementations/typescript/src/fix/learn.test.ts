/**
 * D3: Tests for learnFromBundle (learn.ts).
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync, existsSync, readFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";
import { openDb } from "../db/index.js";
import { principlesLibrary } from "../db/schema/principlesLibrary.js";
import { pendingFixes, fixBundles } from "../db/schema/fixBundles.js";
import { learnFromBundle } from "./learn.js";
import type { FixBundle, ApplyResult, PrincipleCandidate, CapabilitySpec } from "./types.js";
import type { Db } from "../db/index.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function openTestDb(dir: string): Db {
  const db = openDb(join(dir, "test.db"));
  migrate(db, { migrationsFolder: "./drizzle" });
  return db;
}

/** Insert a minimal fix_bundles row so bundleId FK is satisfiable. */
function seedBundleRow(db: Db): number {
  const rows = db
    .insert(fixBundles)
    .values({
      bundleType: "fix",
      createdAt: Date.now(),
      signalRawtext: "test signal",
      signalSource: "test",
      signalSummary: "test summary",
      primaryLayer: "code",
      locusFile: "src/foo.ts",
      locusLine: 1,
      locusPrimaryNode: null,
      appliedAt: null,
      commitSha: null,
      confidence: 0.9,
    })
    .returning({ id: fixBundles.id })
    .all();
  return rows[0].id;
}

function makeApplyResult(applied = true): ApplyResult {
  return { applied, commitSha: applied ? "abc123" : undefined };
}

function makePrinciple(
  overrides: Partial<Extract<PrincipleCandidate, { kind: "principle" }>> = {},
): Extract<PrincipleCandidate, { kind: "principle" }> {
  return {
    kind: "principle",
    name: "null-deref-guard",
    bugClassId: "null-deref-guard",
    dslSource: "(rule null_deref_guard ...)",
    smtTemplate: "(assert (not (= x null)))",
    teachingExample: {
      domain: "null safety",
      explanation: "Dereferencing before null check",
      smt2: "(check-sat)",
    },
    adversarialValidation: [],
    latentSiteMatches: [],
    ...overrides,
  };
}

function makeCapabilitySpec(): CapabilitySpec {
  return {
    capabilityName: "my-capability",
    schemaTs: "// schema",
    migrationSql: "-- migration",
    extractorTs: "// extractor",
    extractorTestsTs: "// tests",
    registryRegistration: "// reg",
    positiveFixtures: [],
    negativeFixtures: [],
    rationale: "test",
  };
}

function makeBundle(overrides: Partial<FixBundle> = {}): FixBundle {
  return {
    bundleId: 1,
    bundleType: "fix",
    bugSignal: {
      source: "test",
      rawText: "TypeError",
      summary: "null deref",
      failureDescription: "crash",
      codeReferences: [],
    },
    plan: {
      signal: {
        source: "test",
        rawText: "TypeError",
        summary: "null deref",
        failureDescription: "crash",
        codeReferences: [],
      },
      locus: null,
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
    confidence: 0.9,
    auditTrail: [],
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("learnFromBundle (D3)", () => {
  let tmpDir: string;
  let db: Db;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), "provekit-learn-test-"));
    db = openTestDb(tmpDir);
  });

  afterEach(() => {
    db.$client.close();
    rmSync(tmpDir, { recursive: true, force: true });
  });

  // -------------------------------------------------------------------------
  // Test 1: Principle added happy path
  // -------------------------------------------------------------------------
  it("writes DSL + JSON files and inserts principles_library row when principle present", async () => {
    const bundleId = seedBundleRow(db);
    const principle = makePrinciple();
    const bundle = makeBundle({
      bundleId,
      artifacts: { primaryFix: null, complementary: [], test: null, principle, capabilitySpec: null },
    });

    const result = await learnFromBundle({
      bundle,
      applyResult: makeApplyResult(true),
      db,
      projectRoot: tmpDir,
    });

    expect(result.principleAdded).toBe("null-deref-guard");
    expect(result.principleFilesWritten).not.toBeNull();

    const dslPath = result.principleFilesWritten!.dslPath;
    const jsonPath = result.principleFilesWritten!.jsonPath;

    expect(existsSync(dslPath)).toBe(true);
    expect(existsSync(jsonPath)).toBe(true);
    expect(readFileSync(dslPath, "utf8")).toBe(principle.dslSource);

    const parsed = JSON.parse(readFileSync(jsonPath, "utf8"));
    expect(parsed.id).toBe("null-deref-guard");
    expect(parsed.confidence).toBe("advisory");

    const rows = db.select().from(principlesLibrary).all();
    expect(rows).toHaveLength(1);
    expect(rows[0].name).toBe("null-deref-guard");
    expect(rows[0].confidenceTier).toBe("advisory");
    expect(rows[0].addedBundleId).toBe(bundleId);
  });

  // -------------------------------------------------------------------------
  // Test 2: No principle in bundle
  // -------------------------------------------------------------------------
  it("does not write files when bundle has no principle", async () => {
    const bundleId = seedBundleRow(db);
    const bundle = makeBundle({ bundleId });

    const result = await learnFromBundle({
      bundle,
      applyResult: makeApplyResult(true),
      db,
      projectRoot: tmpDir,
    });

    expect(result.principleAdded).toBeNull();
    expect(result.principleFilesWritten).toBeNull();

    const rows = db.select().from(principlesLibrary).all();
    expect(rows).toHaveLength(0);
  });

  // -------------------------------------------------------------------------
  // Test 3: Not-applied bundle is a no-op
  // -------------------------------------------------------------------------
  it("is a no-op when applyResult.applied is false", async () => {
    const bundleId = seedBundleRow(db);
    const bundle = makeBundle({
      bundleId,
      artifacts: {
        primaryFix: null,
        complementary: [],
        test: null,
        principle: makePrinciple(),
        capabilitySpec: null,
      },
    });

    const result = await learnFromBundle({
      bundle,
      applyResult: makeApplyResult(false),
      db,
      projectRoot: tmpDir,
    });

    expect(result.principleAdded).toBeNull();
    expect(result.principleFilesWritten).toBeNull();
    expect(result.capabilityRegistered).toBeNull();
    expect(result.pendingFixesEnqueued).toBe(0);
    expect(result.auditLogged).toBe(false);

    const rows = db.select().from(principlesLibrary).all();
    expect(rows).toHaveLength(0);
  });

  // -------------------------------------------------------------------------
  // Test 4: Substrate bundle sets capabilityRegistered
  // -------------------------------------------------------------------------
  it("sets capabilityRegistered for substrate bundles with capabilitySpec", async () => {
    const bundleId = seedBundleRow(db);
    const capabilitySpec = makeCapabilitySpec();
    const bundle = makeBundle({
      bundleId,
      bundleType: "substrate",
      artifacts: {
        primaryFix: null,
        complementary: [],
        test: null,
        principle: null,
        capabilitySpec,
      },
    });

    const result = await learnFromBundle({
      bundle,
      applyResult: makeApplyResult(true),
      db,
      projectRoot: tmpDir,
    });

    expect(result.capabilityRegistered).toBe("my-capability");
  });

  // -------------------------------------------------------------------------
  // Test 5: Latent sites enqueue to pending_fixes
  // -------------------------------------------------------------------------
  it("enqueues 3 pending_fixes rows for 3 latent site matches", async () => {
    const bundleId = seedBundleRow(db);
    const principle = makePrinciple({
      latentSiteMatches: [
        { nodeId: "node-A", file: "src/a.ts", line: 10 },
        { nodeId: "node-B", file: "src/b.ts", line: 20 },
        { nodeId: "node-C", file: "src/c.ts", line: 30 },
      ],
    });
    const bundle = makeBundle({
      bundleId,
      artifacts: { primaryFix: null, complementary: [], test: null, principle, capabilitySpec: null },
    });

    const result = await learnFromBundle({
      bundle,
      applyResult: makeApplyResult(true),
      db,
      projectRoot: tmpDir,
    });

    expect(result.pendingFixesEnqueued).toBe(3);

    const rows = db.select().from(pendingFixes).all();
    expect(rows).toHaveLength(3);
    expect(rows.map((r) => r.siteFile)).toContain("src/a.ts");
    expect(rows.map((r) => r.siteFile)).toContain("src/b.ts");
    expect(rows.map((r) => r.siteFile)).toContain("src/c.ts");
  });

  // -------------------------------------------------------------------------
  // Test 6: New principle starts at confidence_tier "advisory"
  // -------------------------------------------------------------------------
  it("inserts principles_library row with confidence_tier 'advisory'", async () => {
    const bundleId = seedBundleRow(db);
    const bundle = makeBundle({
      bundleId,
      artifacts: {
        primaryFix: null,
        complementary: [],
        test: null,
        principle: makePrinciple(),
        capabilitySpec: null,
      },
    });

    await learnFromBundle({
      bundle,
      applyResult: makeApplyResult(true),
      db,
      projectRoot: tmpDir,
    });

    const rows = db.select().from(principlesLibrary).all();
    expect(rows[0].confidenceTier).toBe("advisory");
    expect(rows[0].falseNegativeCount).toBe(0);
    expect(rows[0].successfulApplicationCount).toBe(0);
  });

  // -------------------------------------------------------------------------
  // Test 7: File writes go to .provekit/principles/
  // -------------------------------------------------------------------------
  it("writes principle files under <projectRoot>/.provekit/principles/", async () => {
    const bundleId = seedBundleRow(db);
    const principle = makePrinciple({ name: "my-safe-principle" });
    const bundle = makeBundle({
      bundleId,
      artifacts: { primaryFix: null, complementary: [], test: null, principle, capabilitySpec: null },
    });

    const result = await learnFromBundle({
      bundle,
      applyResult: makeApplyResult(true),
      db,
      projectRoot: tmpDir,
    });

    const expectedDsl = join(tmpDir, ".provekit", "principles", "my-safe-principle.dsl");
    const expectedJson = join(tmpDir, ".provekit", "principles", "my-safe-principle.json");

    expect(result.principleFilesWritten!.dslPath).toBe(expectedDsl);
    expect(result.principleFilesWritten!.jsonPath).toBe(expectedJson);
    expect(existsSync(expectedDsl)).toBe(true);
    expect(existsSync(expectedJson)).toBe(true);
  });

  // -------------------------------------------------------------------------
  // Test 8: FK SET NULL — deleting bundle does not cascade-delete principles_library
  // -------------------------------------------------------------------------
  it("principles_library row survives bundle deletion (addedBundleId SET NULL)", async () => {
    const bundleId = seedBundleRow(db);
    const bundle = makeBundle({
      bundleId,
      artifacts: {
        primaryFix: null,
        complementary: [],
        test: null,
        principle: makePrinciple(),
        capabilitySpec: null,
      },
    });

    await learnFromBundle({
      bundle,
      applyResult: makeApplyResult(true),
      db,
      projectRoot: tmpDir,
    });

    // Delete the parent bundle row
    db.delete(fixBundles).where(eq(fixBundles.id, bundleId)).run();

    // principles_library row must still exist, with addedBundleId set to null
    const rows = db.select().from(principlesLibrary).all();
    expect(rows).toHaveLength(1);
    expect(rows[0].addedBundleId).toBeNull();
  });
});
