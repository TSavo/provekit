/**
 * Leak 2 substrate prerequisite: chained data-flow tests.
 *
 * Verifies that data_flow_transitive forms real multi-hop chains after the
 * "init"-edge emission added to dataFlow.ts. Without those edges the closure
 * was bipartite (decl→use direct edges only) and chains never formed; with
 * them, a use of a chained variable transitively reaches its original source.
 *
 * The schema interpretation: data_flow rows are (to_node, from_node) where
 * from_node's value flows TO to_node. data_flow_transitive preserves that
 * direction. So "param `a` reaches return-use `y`" is encoded as a row with
 * to_node = use_y_in_return AND from_node = decl_of_a.
 */

import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";
import { openDb } from "../db/index.js";
import { buildSASTForFile } from "./builder.js";
import { nodes as nodesTable, nodeBinding, dataFlowTransitive } from "./schema/index.js";

function openTestDb() {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-df-chains-"));
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

type Db = ReturnType<typeof openDb>;

/**
 * Find all node IDs in the file that match a binding-name AND a kind filter.
 * Used to locate the declaration of a variable.
 */
function findBindingDeclIds(db: Db, name: string): string[] {
  return db
    .select({ id: nodeBinding.nodeId })
    .from(nodeBinding)
    .where(eq(nodeBinding.name, name))
    .all()
    .map((r) => r.id);
}

/**
 * Verify a transitive edge exists from any node matching `fromName` (binding
 * decl) to any identifier-use whose source position falls in [useStart, useEnd].
 * Returns the matching transitive rows for diagnostics.
 */
function findTransitiveEdgesFromDeclToUseInRange(
  db: Db,
  fromName: string,
  useStart: number,
  useEnd: number,
): { toNode: string; fromNode: string }[] {
  const declIds = findBindingDeclIds(db, fromName);
  if (declIds.length === 0) return [];

  const rows = db
    .select({ toNode: dataFlowTransitive.toNode, fromNode: dataFlowTransitive.fromNode })
    .from(dataFlowTransitive)
    .innerJoin(nodesTable, eq(nodesTable.id, dataFlowTransitive.toNode))
    .where(eq(nodesTable.kind, "Identifier"))
    .all();

  // Use sourceEnd (post-trivia end of node) as the reliable anchor.
  // sourceStart in the schema is getFullStart() which includes leading trivia,
  // so the identifier's recorded start may sit on whitespace before the name.
  return rows.filter((r) => {
    if (!declIds.includes(r.fromNode)) return false;
    const toRow = db
      .select({ start: nodesTable.sourceStart, end: nodesTable.sourceEnd })
      .from(nodesTable)
      .where(eq(nodesTable.id, r.toNode))
      .get();
    if (!toRow) return false;
    // The identifier whose textual end position equals useEnd and whose
    // fullStart is at or before useStart (sourceStart includes leading trivia).
    return toRow.end === useEnd && toRow.start <= useStart;
  });
}

describe("data_flow_transitive chain formation", () => {
  let tmpDir: string;
  let db: ReturnType<typeof openDb>;

  afterEach(() => {
    try { db.$client.close(); } catch { /* ignore */ }
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  // -----------------------------------------------------------------------
  // Canonical chain fixture from the plan: param a → x → y → return y
  // -----------------------------------------------------------------------
  it("forms chain: param `a` reaches `return y` through const x = a; const y = x", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function f(a) { const x = a; const y = x; return y; }";
    const filePath = writeFixture(tmpDir, source);
    buildSASTForFile(db, filePath);

    // The "return y" use-site is the y identifier inside the return statement.
    // Find it by source position: it occurs after "return " — second-to-last char run.
    const returnIdx = source.indexOf("return y");
    const yUseStart = returnIdx + "return ".length; // start of `y`
    const yUseEnd = yUseStart + 1;                  // end of `y` (single-char)

    const edges = findTransitiveEdgesFromDeclToUseInRange(db, "a", yUseStart, yUseEnd);
    expect(
      edges.length,
      `expected transitive edge from param decl 'a' to use-of-y in 'return y' (range ${yUseStart}..${yUseEnd})`,
    ).toBeGreaterThan(0);
  });

  // -----------------------------------------------------------------------
  // 4-hop assignment chain: x = a; y = x; z = y; use(z)
  // -----------------------------------------------------------------------
  it("forms chain through assignment statements: a → x → y → z → use(z)", () => {
    ({ db, tmpDir } = openTestDb());
    const source =
      "declare function use(v: any): void;\n" +
      "function f(a) { let x; x = a; let y; y = x; let z; z = y; use(z); }";
    const filePath = writeFixture(tmpDir, source);
    buildSASTForFile(db, filePath);

    // The use(z) identifier z is the last z in the source.
    const useIdx = source.lastIndexOf("use(z)");
    const zUseStart = useIdx + "use(".length;
    const zUseEnd = zUseStart + 1;

    const edges = findTransitiveEdgesFromDeclToUseInRange(db, "a", zUseStart, zUseEnd);
    expect(
      edges.length,
      `expected transitive edge from param decl 'a' to use-of-z in 'use(z)' (range ${zUseStart}..${zUseEnd})`,
    ).toBeGreaterThan(0);
  });

  // -----------------------------------------------------------------------
  // Mixed VariableDeclaration + assignment chain
  // -----------------------------------------------------------------------
  it("forms chain through mixed const-init and assignment: const x = a; y = x", () => {
    ({ db, tmpDir } = openTestDb());
    const source =
      "declare function sink(v: any): void;\n" +
      "function f(a) { const x = a; let y; y = x; sink(y); }";
    const filePath = writeFixture(tmpDir, source);
    buildSASTForFile(db, filePath);

    const sinkIdx = source.lastIndexOf("sink(y)");
    const yUseStart = sinkIdx + "sink(".length;
    const yUseEnd = yUseStart + 1;

    const edges = findTransitiveEdgesFromDeclToUseInRange(db, "a", yUseStart, yUseEnd);
    expect(edges.length, "param `a` should transitively reach `sink(y)`'s y-use").toBeGreaterThan(0);
  });

  // -----------------------------------------------------------------------
  // Interprocedural: NOT expected to chain (documented limitation)
  // -----------------------------------------------------------------------
  it("does NOT chain across function calls (interprocedural unmodeled in v1)", () => {
    ({ db, tmpDir } = openTestDb());
    // outer's `a` is passed to inner; inner's `b` is consumed.
    // We expect param `a` does NOT reach the use of `b` inside `inner`,
    // because there is no call-arg → callee-param edge type. This is the
    // documented v1 boundary; revisit for true taint tracking.
    const source =
      "declare function use(v: any): void;\n" +
      "function inner(b) { use(b); }\n" +
      "function outer(a) { inner(a); }";
    const filePath = writeFixture(tmpDir, source);
    buildSASTForFile(db, filePath);

    // The use(b) identifier b is inside `inner`.
    const innerUseIdx = source.indexOf("use(b)");
    const bUseStart = innerUseIdx + "use(".length;
    const bUseEnd = bUseStart + 1;

    const edges = findTransitiveEdgesFromDeclToUseInRange(db, "a", bUseStart, bUseEnd);
    expect(
      edges.length,
      "param `a` of outer should NOT reach use-of-b in inner (no interprocedural model)",
    ).toBe(0);
  });
});
