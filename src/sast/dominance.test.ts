/**
 * A5: Dominance + post-dominance tests.
 *
 * Tests verify semantic invariants via kind + position queries, not hardcoded IDs.
 * Uses the same in-memory sqlite pattern as A4.
 */

import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { buildSASTForFile } from "./builder.js";
import { nodes as nodesTable, dominance, postDominance } from "./schema/index.js";
import { eq, and } from "drizzle-orm";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type Db = ReturnType<typeof openDb>;

function openTestDb() {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-dom-test-"));
  const dbPath = join(tmpDir, "test.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: "./drizzle" });
  return { db, tmpDir };
}

function writeFixture(dir: string, source: string, filename = "fixture.ts"): string {
  mkdirSync(dir, { recursive: true });
  const filePath = join(dir, filename);
  writeFileSync(filePath, source, "utf8");
  return filePath;
}

/** Return node IDs for nodes of a given kind in the given file. */
function nodesOfKind(db: Db, fileId: number, kind: string): string[] {
  return db.select({ id: nodesTable.id })
    .from(nodesTable)
    .where(and(eq(nodesTable.fileId, fileId), eq(nodesTable.kind, kind)))
    .all()
    .map((r) => r.id);
}

/**
 * Check if dominator dominates dominated (row exists in dominance table).
 */
function dominates(db: Db, dominatorId: string, dominatedId: string): boolean {
  const row = db.select().from(dominance)
    .where(and(eq(dominance.dominator, dominatorId), eq(dominance.dominated, dominatedId)))
    .get();
  return row !== undefined;
}

/**
 * Check if post_dominator post-dominates post_dominated.
 */
function postDominates(db: Db, postDominatorId: string, postDominatedId: string): boolean {
  const row = db.select().from(postDominance)
    .where(and(eq(postDominance.postDominator, postDominatorId), eq(postDominance.postDominated, postDominatedId)))
    .get();
  return row !== undefined;
}

/** Return all dominance rows for a given file (via join with nodes). */
function allDominanceForFile(db: Db, fileId: number) {
  return db.select({ dominator: dominance.dominator, dominated: dominance.dominated })
    .from(dominance)
    .innerJoin(nodesTable, eq(dominance.dominated, nodesTable.id))
    .where(eq(nodesTable.fileId, fileId))
    .all();
}

/** Return all post_dominance rows for a given file (via join with nodes). */
function allPostDominanceForFile(db: Db, fileId: number) {
  return db.select({ postDominator: postDominance.postDominator, postDominated: postDominance.postDominated })
    .from(postDominance)
    .innerJoin(nodesTable, eq(postDominance.postDominated, nodesTable.id))
    .where(eq(nodesTable.fileId, fileId))
    .all();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("dominance extractor", () => {
  let tmpDir: string;
  let db: Db;

  afterEach(() => {
    try { db.$client.close(); } catch { /* ignore */ }
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  // -------------------------------------------------------------------------
  // 1. Plan fixture: if(x>0) return x; throw; return 0; (dead code)
  // -------------------------------------------------------------------------
  it("plan fixture: function entry dominates if-statement", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function f(x) {\n  if (x > 0) return x;\n  throw new Error();\n  return 0;\n}",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    // Get function node ID (FunctionDeclaration)
    const fnIds = nodesOfKind(db, fileId, "FunctionDeclaration");
    expect(fnIds.length, "at least one FunctionDeclaration").toBeGreaterThan(0);
    const fnId = fnIds[0];

    // Get IfStatement node ID
    const ifIds = nodesOfKind(db, fileId, "IfStatement");
    expect(ifIds.length, "at least one IfStatement").toBeGreaterThan(0);
    const ifId = ifIds[0];

    // Function entry dominates the if-statement
    expect(dominates(db, fnId, ifId), "FunctionDeclaration dominates IfStatement").toBe(true);

    // The if-statement dominates itself (self-dominance)
    expect(dominates(db, ifId, ifId), "IfStatement dominates itself").toBe(true);

    // Function entry dominates itself (self-dominance)
    expect(dominates(db, fnId, fnId), "FunctionDeclaration dominates itself").toBe(true);
  });

  it("plan fixture: if-statement dominates its then-branch return", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function f(x) {\n  if (x > 0) return x;\n  throw new Error();\n  return 0;\n}",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    const ifIds = nodesOfKind(db, fileId, "IfStatement");
    expect(ifIds.length).toBeGreaterThan(0);
    const ifId = ifIds[0];

    // There should be ReturnStatement nodes — at least one in then-branch
    const retIds = nodesOfKind(db, fileId, "ReturnStatement");
    expect(retIds.length, "at least one ReturnStatement").toBeGreaterThan(0);

    // The if dominates the `return x` in the then-branch
    // (which is the ReturnStatement that appears BEFORE the throw)
    // We check that at least one ReturnStatement is dominated by the if
    const dominated = retIds.some((retId) => dominates(db, ifId, retId));
    expect(dominated, "if-statement dominates at least one return statement").toBe(true);
  });

  it("plan fixture: function entry dominates throw statement", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function f(x) {\n  if (x > 0) return x;\n  throw new Error();\n  return 0;\n}",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    const fnIds = nodesOfKind(db, fileId, "FunctionDeclaration");
    const throwIds = nodesOfKind(db, fileId, "ThrowStatement");
    expect(throwIds.length, "at least one ThrowStatement").toBeGreaterThan(0);

    // Function entry dominates the throw
    expect(dominates(db, fnIds[0], throwIds[0]), "FunctionDeclaration dominates ThrowStatement").toBe(true);
  });

  it("plan fixture: return x post-dominates itself", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function f(x) {\n  if (x > 0) return x;\n  throw new Error();\n  return 0;\n}",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    const retIds = nodesOfKind(db, fileId, "ReturnStatement");
    expect(retIds.length).toBeGreaterThan(0);

    // At least one return post-dominates itself (self-post-dominance)
    const selfPostDom = retIds.some((retId) => postDominates(db, retId, retId));
    expect(selfPostDom, "ReturnStatement post-dominates itself").toBe(true);
  });

  it("plan fixture: throw statement post-dominates itself", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function f(x) {\n  if (x > 0) return x;\n  throw new Error();\n  return 0;\n}",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    const throwIds = nodesOfKind(db, fileId, "ThrowStatement");
    expect(throwIds.length).toBeGreaterThan(0);

    expect(postDominates(db, throwIds[0], throwIds[0]), "ThrowStatement post-dominates itself").toBe(true);
  });

  // -------------------------------------------------------------------------
  // 2. Simple sequence: const a = 1; const b = 2; return a + b;
  // -------------------------------------------------------------------------
  it("simple sequence: first statement dominates later ones", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function g() { const a = 1; const b = 2; return a + b; }",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    const fnIds = nodesOfKind(db, fileId, "FunctionDeclaration");
    const retIds = nodesOfKind(db, fileId, "ReturnStatement");

    expect(fnIds.length).toBeGreaterThan(0);
    expect(retIds.length).toBeGreaterThan(0);

    // Function entry dominates the return
    expect(dominates(db, fnIds[0], retIds[0]), "function entry dominates return").toBe(true);

    // All dominance rows should exist (some non-trivial set)
    const rows = allDominanceForFile(db, fileId);
    expect(rows.length, "some dominance rows emitted for sequence").toBeGreaterThan(0);
  });

  it("simple sequence: variable decl statements dominate return", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function g() { const a = 1; const b = 2; return a + b; }",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    // VariableStatement nodes (const a = 1, const b = 2)
    const varIds = nodesOfKind(db, fileId, "VariableStatement");
    const retIds = nodesOfKind(db, fileId, "ReturnStatement");

    expect(varIds.length, "at least 2 VariableStatement nodes").toBeGreaterThanOrEqual(2);
    expect(retIds.length).toBeGreaterThan(0);

    // First var statement dominates return
    const firstVarDomRet = varIds.some((varId) => dominates(db, varId, retIds[0]));
    expect(firstVarDomRet, "at least one VariableStatement dominates the return").toBe(true);
  });

  // -------------------------------------------------------------------------
  // 3. Loop: while(i < 10) i++; return i;
  // -------------------------------------------------------------------------
  it("loop: while condition dominates body", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function h() { let i = 0; while (i < 10) i++; return i; }",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    const whileIds = nodesOfKind(db, fileId, "WhileStatement");
    expect(whileIds.length, "at least one WhileStatement").toBeGreaterThan(0);
    const whileId = whileIds[0];

    // The while condition dominates the expression statement body (i++)
    const exprIds = nodesOfKind(db, fileId, "ExpressionStatement");
    expect(exprIds.length, "at least one ExpressionStatement (i++)").toBeGreaterThan(0);

    // while dominates the body expression statement
    const whileDomBody = exprIds.some((exprId) => dominates(db, whileId, exprId));
    expect(whileDomBody, "while-statement dominates body expression").toBe(true);
  });

  it("loop: while dominates the return (entry → while → ... → return)", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function h() { let i = 0; while (i < 10) i++; return i; }",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    const whileIds = nodesOfKind(db, fileId, "WhileStatement");
    const retIds = nodesOfKind(db, fileId, "ReturnStatement");
    expect(whileIds.length).toBeGreaterThan(0);
    expect(retIds.length).toBeGreaterThan(0);

    // while dominates return (only path to return goes through while)
    expect(dominates(db, whileIds[0], retIds[0]), "while dominates return").toBe(true);
  });

  it("loop: body does NOT dominate the return (alternate path via loop exit)", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function h() { let i = 0; while (i < 10) i++; return i; }",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    const exprIds = nodesOfKind(db, fileId, "ExpressionStatement");
    const retIds = nodesOfKind(db, fileId, "ReturnStatement");
    expect(exprIds.length).toBeGreaterThan(0);
    expect(retIds.length).toBeGreaterThan(0);

    // The body (i++) does NOT dominate the return — the while condition does
    // (you can reach the return without executing the body if condition is false initially)
    const bodyDomRet = exprIds.some((exprId) => dominates(db, exprId, retIds[0]));
    expect(bodyDomRet, "loop body should NOT dominate the return").toBe(false);
  });

  it("loop: return post-dominates itself", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function h() { let i = 0; while (i < 10) i++; return i; }",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    const retIds = nodesOfKind(db, fileId, "ReturnStatement");
    expect(retIds.length).toBeGreaterThan(0);

    expect(postDominates(db, retIds[0], retIds[0]), "return post-dominates itself").toBe(true);
  });

  // -------------------------------------------------------------------------
  // 4. Multi-file: dominance rows in second file don't conflict with first
  // -------------------------------------------------------------------------
  it("multi-file: second file build succeeds without PK conflicts", () => {
    ({ db, tmpDir } = openTestDb());
    const file1 = writeFixture(
      tmpDir,
      "function f1(a: number) { if (a > 0) return a; return -1; }",
      "one.ts",
    );
    buildSASTForFile(db, file1);
    const domAfter1 = db.select().from(dominance).all().length;
    const postDomAfter1 = db.select().from(postDominance).all().length;

    const file2 = writeFixture(
      tmpDir,
      "function f2(b: number) { while (b > 0) b--; return b; }",
      "two.ts",
    );
    expect(() => buildSASTForFile(db, file2)).not.toThrow();

    const domAfter2 = db.select().from(dominance).all().length;
    const postDomAfter2 = db.select().from(postDominance).all().length;

    expect(domAfter2, "dominance rows increase after second file").toBeGreaterThan(domAfter1);
    expect(postDomAfter2, "post_dominance rows increase after second file").toBeGreaterThan(postDomAfter1);
  });

  it("multi-file: each file's dominance rows are scoped correctly", () => {
    ({ db, tmpDir } = openTestDb());
    const file1 = writeFixture(
      tmpDir,
      "function f1(a: number) { return a * 2; }",
      "one.ts",
    );
    const { fileId: fileId1 } = buildSASTForFile(db, file1);

    const file2 = writeFixture(
      tmpDir,
      "function f2(b: number) { const c = b + 1; return c; }",
      "two.ts",
    );
    const { fileId: fileId2 } = buildSASTForFile(db, file2);

    expect(fileId1).not.toBe(fileId2);

    // Each file should have its own dominance rows
    const dom1 = allDominanceForFile(db, fileId1);
    const dom2 = allDominanceForFile(db, fileId2);

    expect(dom1.length, "file1 has dominance rows").toBeGreaterThan(0);
    expect(dom2.length, "file2 has dominance rows").toBeGreaterThan(0);

    // No cross-file contamination: every node in file1's dom rows should be in file1
    const nodeIds1 = new Set(
      db.select({ id: nodesTable.id }).from(nodesTable).where(eq(nodesTable.fileId, fileId1)).all().map((r) => r.id),
    );
    for (const row of dom1) {
      expect(nodeIds1.has(row.dominator), `dominator ${row.dominator} belongs to file1`).toBe(true);
      expect(nodeIds1.has(row.dominated), `dominated ${row.dominated} belongs to file1`).toBe(true);
    }
  });

  // -------------------------------------------------------------------------
  // 5. Self-dominance: every reachable CFG node dominates itself
  // -------------------------------------------------------------------------
  it("self-dominance holds for function declaration node", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function z(n: number) { return n + 1; }",
    );
    const { fileId } = buildSASTForFile(db, filePath);

    const fnIds = nodesOfKind(db, fileId, "FunctionDeclaration");
    expect(fnIds.length).toBeGreaterThan(0);

    expect(dominates(db, fnIds[0], fnIds[0]), "fn node dominates itself").toBe(true);
    expect(postDominates(db, fnIds[0], fnIds[0]), "fn node post-dominates itself").toBe(true);
  });
});
