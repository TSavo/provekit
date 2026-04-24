/**
 * D1b: Tests for oracle runners.
 *
 * One test per oracle. Uses minimal fixtures and in-memory SQLite.
 * Stubs (7, 12, 15 MVP) just verify the function returns {passed: true}.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../db/index.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import type { Db } from "../db/index.js";
import type { OverlayHandle, InvariantClaim, FixCandidate } from "./types.js";
import {
  runOracle4,
  runOracle5,
  runOracle7,
  runOracle8,
  runOracle10,
  runOracle11,
  runOracle12,
  runOracle13,
  runOracle15,
} from "./oracles.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function openTestDb(): Db {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-oracle-test-"));
  const db = openDb(join(tmpDir, "test.db"));
  migrate(db, { migrationsFolder: "./drizzle" });
  return db;
}

function makeFakeOverlay(sastDb: Db): OverlayHandle {
  return {
    worktreePath: "/tmp/fake-overlay",
    sastDbPath: "/tmp/fake-overlay.db",
    sastDb,
    baseRef: "HEAD",
    modifiedFiles: new Set(),
    closed: false,
  };
}

function makeInvariantClaim(formalExpression = "(declare-const x Int)\n(assert (> x 0))\n(check-sat)\n"): InvariantClaim {
  return {
    principleId: null,
    description: "x must be positive",
    formalExpression,
    bindings: [],
    complexity: 1,
    witness: null,
  };
}

function makeFixCandidate(): FixCandidate {
  return {
    patch: { fileEdits: [{ file: "src/foo.ts", newContent: "export const x = 1;" }], description: "fix" },
    llmRationale: "guard added",
    llmConfidence: 0.85,
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "unsat",
    audit: {
      overlayCreated: true,
      patchApplied: true,
      overlayReindexed: true,
      z3RunMs: 100,
      overlayClosed: false,
    },
  };
}

// ---------------------------------------------------------------------------
// Oracle #4 — no-regression on proven clauses
// ---------------------------------------------------------------------------

describe("runOracle4", () => {
  let db: Db;
  let tmpDirs: string[] = [];

  beforeEach(() => {
    db = openTestDb();
  });

  afterEach(() => {
    db.$client.close();
    for (const d of tmpDirs) {
      try { rmSync(d, { recursive: true, force: true }); } catch { /* ignore */ }
    }
    tmpDirs = [];
  });

  it("passes when clauses table has no proven rows", async () => {
    const overlay = makeFakeOverlay(db);
    const result = await runOracle4({ overlay, mainDb: db });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/no prior proven clauses|clauses table not accessible/);
  });

  it("passes when mainDb clauses table is not accessible (fresh test fixture)", async () => {
    // Use a fresh in-memory DB with no clauses table
    const freshDb = openDb(":memory:");
    const overlay = makeFakeOverlay(db);
    const result = await runOracle4({ overlay, mainDb: freshDb });
    expect(result.passed).toBe(true);
    freshDb.$client.close();
  });
});

// ---------------------------------------------------------------------------
// Oracle #5 — bundle coherence SMT
// ---------------------------------------------------------------------------

describe("runOracle5", () => {
  it("passes trivially with 0 invariants", () => {
    const result = runOracle5({ invariants: [] });
    expect(result.passed).toBe(true);
  });

  it("passes trivially with 1 invariant", () => {
    const result = runOracle5({ invariants: [makeInvariantClaim()] });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/single invariant/);
  });

  it("passes with 2 consistent invariants", () => {
    // Both assert x > 0 — satisfiable together
    const inv1 = makeInvariantClaim("(declare-const x Int)\n(assert (> x 0))\n(check-sat)\n");
    const inv2 = makeInvariantClaim("(declare-const y Int)\n(assert (> y 0))\n(check-sat)\n");
    const result = runOracle5({ invariants: [inv1, inv2] });
    // Z3 may not be available in CI; either pass or MVP-leniency pass
    expect(result.passed).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Oracle #7 — witness replay (MVP stub)
// ---------------------------------------------------------------------------

describe("runOracle7", () => {
  it("returns passed:true (MVP stub)", async () => {
    const db = openTestDb();
    const overlay = makeFakeOverlay(db);
    const result = await runOracle7({
      overlay,
      fix: makeFixCandidate(),
      invariant: makeInvariantClaim(),
      witnessInputs: { x: 5 },
    });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/deferred/i);
    db.$client.close();
  });
});

// ---------------------------------------------------------------------------
// Oracle #8 — no-new-gaps
// ---------------------------------------------------------------------------

describe("runOracle8", () => {
  let db: Db;

  beforeEach(() => {
    db = openTestDb();
  });

  afterEach(() => {
    db.$client.close();
  });

  it("passes when gap_reports table not accessible in overlay sastDb", async () => {
    const overlay = makeFakeOverlay(db);
    // db has gap_reports (via migration) but overlay.sastDb also points to db
    // The overlay's sastDb is the same db in this test — gap_reports exists but is empty.
    const result = await runOracle8({ overlay, mainDb: db });
    expect(result.passed).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Oracle #10 — full suite with retry-once
// ---------------------------------------------------------------------------

describe("runOracle10", () => {
  it("passes immediately when runner returns exitCode 0", async () => {
    const db = openTestDb();
    const overlay = makeFakeOverlay(db);
    const runner = () => ({ exitCode: 0, stdout: "all tests passed", stderr: "" });
    const result = await runOracle10({ overlay, runner });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/first run/);
    db.$client.close();
  });

  it("passes on second run (flake) and notes it", async () => {
    const db = openTestDb();
    const overlay = makeFakeOverlay(db);
    let callCount = 0;
    const runner = () => {
      callCount++;
      return callCount === 1
        ? { exitCode: 1, stdout: "", stderr: "timeout" }
        : { exitCode: 0, stdout: "all good", stderr: "" };
    };
    const result = await runOracle10({ overlay, runner });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/flake/i);
    expect(callCount).toBe(2);
    db.$client.close();
  });

  it("fails when both runs fail", async () => {
    const db = openTestDb();
    const overlay = makeFakeOverlay(db);
    const runner = () => ({ exitCode: 1, stdout: "", stderr: "test failure" });
    const result = await runOracle10({ overlay, runner });
    expect(result.passed).toBe(false);
    expect(result.detail).toMatch(/both runs/);
    db.$client.close();
  });
});

// ---------------------------------------------------------------------------
// Oracle #11 — SAST structural coherence
// ---------------------------------------------------------------------------

describe("runOracle11", () => {
  it("fails when nodes table is empty", async () => {
    const db = openTestDb();
    // The main DB migration doesn't include SAST tables; create an in-memory SAST-like DB.
    const sastDb = openDb(":memory:");
    // Create minimal nodes table
    sastDb.$client.exec("CREATE TABLE nodes (id TEXT PRIMARY KEY)");
    const overlay = makeFakeOverlay(sastDb);
    const result = await runOracle11({ overlay });
    expect(result.passed).toBe(false);
    expect(result.detail).toMatch(/0 nodes/);
    sastDb.$client.close();
    db.$client.close();
  });

  it("passes when nodes table has rows", async () => {
    const db = openTestDb();
    const sastDb = openDb(":memory:");
    sastDb.$client.exec("CREATE TABLE nodes (id TEXT PRIMARY KEY)");
    sastDb.$client.exec("CREATE TABLE node_children (parent_id TEXT, child_id TEXT)");
    sastDb.$client.exec("INSERT INTO nodes VALUES ('node-1')");
    const overlay = makeFakeOverlay(sastDb);
    const result = await runOracle11({ overlay });
    expect(result.passed).toBe(true);
    sastDb.$client.close();
    db.$client.close();
  });
});

// ---------------------------------------------------------------------------
// Oracle #12 — DSL no silent regressions (MVP stub)
// ---------------------------------------------------------------------------

describe("runOracle12", () => {
  it("returns passed:true (MVP stub)", async () => {
    const db = openTestDb();
    const overlay = makeFakeOverlay(db);
    const result = await runOracle12({ overlay, mainDb: db });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/deferred|MVP/i);
    db.$client.close();
  });
});

// ---------------------------------------------------------------------------
// Oracle #13 — gap closure
// ---------------------------------------------------------------------------

describe("runOracle13", () => {
  it("skips gracefully when no triggeringGapId", async () => {
    const db = openTestDb();
    const overlay = makeFakeOverlay(db);
    const result = await runOracle13({ overlay });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/not gap-report-sourced/);
    db.$client.close();
  });

  it("passes (skip) when gap_reports not in overlay sastDb", async () => {
    const db = openTestDb();
    const sastDb = openDb(":memory:");
    // No gap_reports table in this db
    const overlay = makeFakeOverlay(sastDb);
    const result = await runOracle13({ overlay, triggeringGapId: 99 });
    // Should gracefully skip since gap_reports doesn't exist in this SAST DB
    expect(result.passed).toBe(true);
    sastDb.$client.close();
    db.$client.close();
  });
});

// ---------------------------------------------------------------------------
// Oracle #15 — cross-codebase regression (MVP stub)
// ---------------------------------------------------------------------------

describe("runOracle15", () => {
  it("returns passed:true (MVP stub)", async () => {
    const db = openTestDb();
    const overlay = makeFakeOverlay(db);
    const fakeCapSpec = {
      capabilityName: "test",
      schemaTs: "",
      migrationSql: "",
      extractorTs: "",
      extractorTestsTs: "",
      registryRegistration: "",
      positiveFixtures: [],
      negativeFixtures: [],
      rationale: "",
    };
    const result = await runOracle15({ overlay, mainDb: db, capabilitySpec: fakeCapSpec });
    expect(result.passed).toBe(true);
    expect(result.detail).toMatch(/deferred|MVP/i);
    db.$client.close();
  });
});
