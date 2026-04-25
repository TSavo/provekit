/**
 * pathConditions.test.ts
 *
 * Tests for extractGuardConditions — verifying that dominating guards over
 * invariant binding sites are correctly extracted as SMT assertions.
 *
 * These tests build a real SAST DB from fixture source code, run the full
 * extraction pipeline (builder + dominance + capabilities), then call
 * extractGuardConditions and assert on the returned SMT assertions.
 *
 * Integration test at the end ties a guard-based fix to a passing oracle #2 verdict.
 */

import { describe, it, expect, afterEach } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  rmSync,
  writeFileSync,
  cpSync,
  existsSync,
} from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { buildSASTForFile } from "../sast/builder.js";
import { extractGuardConditions } from "./pathConditions.js";
import { runOracleTwo } from "./candidateGen.js";
import { openOverlay } from "./stages/openOverlay.js";
import { applyPatchToOverlay, reindexOverlay, closeOverlay } from "./overlay.js";
import type { InvariantClaim, OverlayHandle } from "./types.js";
import type { SmtBinding } from "../contracts.js";

// ---------------------------------------------------------------------------
// Git config for test commits
// ---------------------------------------------------------------------------

const GIT_ID = ["-c", "user.name=test", "-c", "user.email=test@test"];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeTestRepo(content: string, filename = "fixture.ts"): { repoDir: string; filePath: string } {
  const repoDir = mkdtempSync(join(tmpdir(), "provekit-pc-test-repo-"));
  execFileSync("git", [...GIT_ID, "init", repoDir]);
  execFileSync("git", [...GIT_ID, "init"], { cwd: repoDir });

  const filePath = join(repoDir, filename);
  writeFileSync(filePath, content, "utf8");

  execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
  execFileSync("git", [...GIT_ID, "commit", "-m", "init"], { cwd: repoDir });

  return { repoDir, filePath };
}

function openMainDb(dir: string) {
  const dbPath = join(dir, "main.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: "./drizzle" });
  return { db, dbPath };
}

function makeBinding(smt_constant: string, source_expr: string, sort = "Int"): SmtBinding {
  return { smt_constant, source_expr, sort, source_line: 1 };
}

function makeDivInvariant(extraBindings: SmtBinding[] = []): InvariantClaim {
  return {
    principleId: null,
    description: "Division where denominator may be zero",
    formalExpression:
      "(declare-const b Int)\n(assert (= b 0))\n(check-sat)",
    bindings: [
      makeBinding("b", "b"),
      ...extraBindings,
    ],
    complexity: 1,
    witness: "sat",
  };
}

function makeLocus(filePath: string) {
  return {
    file: filePath,
    line: 1,
    confidence: 1.0,
    primaryNode: "placeholder",
    containingFunction: "placeholder",
    relatedFunctions: [],
    dataFlowAncestors: [],
    dataFlowDescendants: [],
    dominanceRegion: [],
    postDominanceRegion: [],
  };
}

function seedPrinciples(repoDir: string): void {
  const src = join(process.cwd(), ".provekit", "principles");
  const dst = join(repoDir, ".provekit", "principles");
  mkdirSync(dst, { recursive: true });
  if (existsSync(src)) {
    cpSync(src, dst, { recursive: true });
    execFileSync("git", [...GIT_ID, "add", "."], { cwd: repoDir });
    execFileSync("git", [...GIT_ID, "commit", "-m", "add principles"], { cwd: repoDir });
  }
}

// Build an OverlayHandle backed by a fresh scratch DB for the given source.
async function buildOverlayForSource(
  content: string,
  repoDir: string,
  filePath: string,
  mainDb: ReturnType<typeof openDb>,
): Promise<OverlayHandle> {
  buildSASTForFile(mainDb, filePath);

  const locus = makeLocus(filePath);
  // Use a placeholder primaryNode — openOverlay uses the DB we pass.
  locus.primaryNode = "placeholder";

  const overlay = await openOverlay({ locus, db: mainDb });
  return overlay;
}

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

describe("pathConditions: extractGuardConditions", () => {
  const cleanups: (() => void | Promise<void>)[] = [];

  afterEach(async () => {
    for (const fn of cleanups.splice(0)) {
      try { await fn(); } catch { /* ignore */ }
    }
  });

  // -------------------------------------------------------------------------
  // Test 1: binding `b`, dominating guard `if (b !== 0)` in consequent branch
  //   → extract `(assert (not (= b 0)))`
  // -------------------------------------------------------------------------
  it("binding b + dominating if (b !== 0) in consequent → (assert (not (= b 0)))", async () => {
    // Guard is in the THEN branch — the division is guarded by b !== 0.
    const source = `
export function safeDivide(a: number, b: number): number {
  if (b !== 0) {
    return a / b;
  }
  return 0;
}
`.trim() + "\n";

    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));
    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-pc-db1-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));
    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const overlay = await buildOverlayForSource(source, repoDir, filePath, mainDb);
    cleanups.push(() => closeOverlay(overlay));

    const bindings = [makeBinding("b", "b")];
    const result = extractGuardConditions(overlay, bindings);

    // Should find the b !== 0 guard.
    expect(result.guardCount).toBeGreaterThan(0);
    // The assertion should negate b === 0 (i.e., b !== 0 asserted in the consequent).
    const assertionStr = result.smtAssertions.join(" ");
    expect(assertionStr).toContain("(not (= b 0))");
  }, 30_000);

  // -------------------------------------------------------------------------
  // Test 2: binding `b`, no dominating guard → empty array
  // -------------------------------------------------------------------------
  it("binding b + no dominating guard → guardCount 0, empty assertions", async () => {
    const source = `
export function unsafeDivide(a: number, b: number): number {
  return a / b;
}
`.trim() + "\n";

    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-pc-db2-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));
    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);
    const locus = makeLocus(filePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    const bindings = [makeBinding("b", "b")];
    const result = extractGuardConditions(overlay, bindings);

    expect(result.guardCount).toBe(0);
    expect(result.smtAssertions).toHaveLength(0);
  }, 30_000);

  // -------------------------------------------------------------------------
  // Test 3: binding `b`, guard is on different variable `x` → empty
  // -------------------------------------------------------------------------
  it("binding b + guard on different variable x → guardCount 0", async () => {
    const source = `
export function fn(a: number, b: number, x: number): number {
  if (x !== 0) {
    return a / b;
  }
  return 0;
}
`.trim() + "\n";

    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-pc-db3-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));
    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);
    const locus = makeLocus(filePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    // We're looking for guards on "b" — the guard "x !== 0" should not match.
    const bindings = [makeBinding("b", "b")];
    const result = extractGuardConditions(overlay, bindings);

    expect(result.guardCount).toBe(0);
    expect(result.smtAssertions).toHaveLength(0);
  }, 30_000);

  // -------------------------------------------------------------------------
  // Test 4: binding `b`, dominating guard `if (b > 5)` → `(assert (> b 5))`
  // -------------------------------------------------------------------------
  it("binding b + dominating if (b > 5) → (assert (> b 5))", async () => {
    const source = `
export function fn(a: number, b: number): number {
  if (b > 5) {
    return a / b;
  }
  return 0;
}
`.trim() + "\n";

    const { repoDir, filePath } = makeTestRepo(source);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));
    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-pc-db4-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));
    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    const overlay = await buildOverlayForSource(source, repoDir, filePath, mainDb);
    cleanups.push(() => closeOverlay(overlay));

    const bindings = [makeBinding("b", "b")];
    const result = extractGuardConditions(overlay, bindings);

    expect(result.guardCount).toBeGreaterThan(0);
    const assertionStr = result.smtAssertions.join(" ");
    expect(assertionStr).toContain("(> b 5)");
  }, 30_000);

  // -------------------------------------------------------------------------
  // Test 5: Integration — guard-based fix passes oracle #2
  //   Source: function with if (b !== 0) guard added → division still present
  //   but guarded. Proxy rejects (division still matches). Guard path finds
  //   b !== 0 dominates the division use → augmented SMT is unsat → "unsat".
  // -------------------------------------------------------------------------
  it("integration: guard-based fix passes oracle #2 via guard-augmented SMT", async () => {
    // Post-fix source: guard added, division still present.
    const fixedSource = `
export function divide(a: number, b: number): number {
  if (b !== 0) {
    return a / b;
  }
  return 0;
}
`.trim() + "\n";

    const { repoDir, filePath } = makeTestRepo(fixedSource);
    cleanups.push(() => rmSync(repoDir, { recursive: true, force: true }));
    seedPrinciples(repoDir);

    const mainTmp = mkdtempSync(join(tmpdir(), "provekit-pc-db5-"));
    cleanups.push(() => rmSync(mainTmp, { recursive: true, force: true }));
    const { db: mainDb } = openMainDb(mainTmp);
    cleanups.push(() => { try { mainDb.$client.close(); } catch { /* ignore */ } });

    buildSASTForFile(mainDb, filePath);
    const locus = makeLocus(filePath);
    const overlay = await openOverlay({ locus, db: mainDb });
    cleanups.push(() => closeOverlay(overlay));

    // The patch "applied" is the file as-is (already guarded).
    // We simulate the overlay already having the guarded file by applying a no-op patch.
    applyPatchToOverlay(overlay, {
      fileEdits: [{ file: "fixture.ts", newContent: fixedSource }],
      description: "apply guard-based fix",
    });
    await reindexOverlay(overlay);

    // InvariantClaim: violation SMT says b == 0.
    // The guard `if (b !== 0)` makes this violation unreachable → unsat after augmentation.
    const invariant: InvariantClaim = {
      principleId: null,
      description: "Division denominator may be zero",
      formalExpression:
        "(declare-const b Int)\n(assert (= b 0))\n(check-sat)",
      bindings: [makeBinding("b", "b")],
      complexity: 1,
      witness: "sat",
    };

    const verdict = await runOracleTwo(overlay, invariant);

    // Guard `b !== 0` dominates the division use site → augmented SMT:
    //   (declare-const b Int)
    //   (assert (= b 0))      ← violation assertion
    //   (assert (not (= b 0))) ← guard assertion
    //   (check-sat)
    // Z3 returns unsat → oracle #2 should return "unsat".
    expect(verdict).toBe("unsat");
  }, 60_000);
});
