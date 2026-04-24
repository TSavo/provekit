/**
 * A4: Data-flow edge tests (syntactic def-use + transitive closure).
 *
 * Tests verify semantic invariants — "there is an edge from a Parameter node
 * named X to an Identifier node in the right expression context with slot Y" —
 * not specific node IDs.
 */

import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { buildSASTForFile } from "./builder.js";
import { nodes as nodesTable, nodeBinding, dataFlow, dataFlowTransitive } from "./schema/index.js";
import { eq, and } from "drizzle-orm";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function openTestDb() {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-df-test-"));
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
 * Get all data_flow rows joined with to_node and from_node info for assertions.
 */
function getEdges(db: Db) {
  const rows = db.select({
    toNode: dataFlow.toNode,
    fromNode: dataFlow.fromNode,
    slot: dataFlow.slot,
    toKind: nodesTable.kind,
    toStart: nodesTable.sourceStart,
    toEnd: nodesTable.sourceEnd,
  })
    .from(dataFlow)
    .innerJoin(nodesTable, eq(dataFlow.toNode, nodesTable.id))
    .all();

  // Enrich with from_node info
  return rows.map((r) => {
    const fromInfo = db.select({ kind: nodesTable.kind, start: nodesTable.sourceStart })
      .from(nodesTable)
      .where(eq(nodesTable.id, r.fromNode))
      .get();
    return { ...r, fromKind: fromInfo?.kind, fromStart: fromInfo?.start };
  });
}

/**
 * Find edges where the from_node is a named binding (via nodeBinding) with
 * a given name, and the slot matches.
 */
function edgesFromBinding(db: Db, name: string, slot: string) {
  const bindings = db.select({ nodeId: nodeBinding.nodeId })
    .from(nodeBinding)
    .where(eq(nodeBinding.name, name))
    .all();
  const bindingIds = new Set(bindings.map((b) => b.nodeId));

  const edges = db.select().from(dataFlow).all();
  return edges.filter((e) => bindingIds.has(e.fromNode) && e.slot === slot);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("data-flow extractor", () => {
  let tmpDir: string;
  let db: ReturnType<typeof openDb>;

  afterEach(() => {
    try { db.$client.close(); } catch { /* ignore */ }
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  // -----------------------------------------------------------------------
  // 1. Division fixture: a (lhs), b (denominator), q (return)
  // -----------------------------------------------------------------------
  it("emits lhs/denominator/return edges for division fixture", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function f(a, b) { const q = a / b; return q; }",
    );
    buildSASTForFile(db, filePath);

    // a used as LHS of a/b
    const aLhs = edgesFromBinding(db, "a", "lhs");
    expect(aLhs.length, "edge from param 'a' with slot 'lhs'").toBeGreaterThan(0);

    // b used as denominator of /
    const bDenom = edgesFromBinding(db, "b", "denominator");
    expect(bDenom.length, "edge from param 'b' with slot 'denominator'").toBeGreaterThan(0);

    // q used in return
    const qReturn = edgesFromBinding(db, "q", "return");
    expect(qReturn.length, "edge from binding 'q' with slot 'return'").toBeGreaterThan(0);
  });

  // -----------------------------------------------------------------------
  // 2. Condition fixture
  // -----------------------------------------------------------------------
  it("emits condition edge for if(x)", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function g(x) { if (x) return 1; return 0; }",
    );
    buildSASTForFile(db, filePath);

    const cond = edgesFromBinding(db, "x", "condition");
    expect(cond.length, "edge from param 'x' with slot 'condition'").toBeGreaterThan(0);
  });

  // -----------------------------------------------------------------------
  // 3. Call argument fixture — 4 args get arg[0]/arg[1]/arg[2]/arg[n]
  // -----------------------------------------------------------------------
  it("emits arg slot edges for 4-arg call", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "declare function f(a:any,b:any,c:any,d:any):void; function h(a,b,c,d) { return f(a, b, c, d); }",
    );
    buildSASTForFile(db, filePath);

    const arg0 = edgesFromBinding(db, "a", "arg[0]");
    expect(arg0.length, "edge for arg[0]").toBeGreaterThan(0);

    const arg1 = edgesFromBinding(db, "b", "arg[1]");
    expect(arg1.length, "edge for arg[1]").toBeGreaterThan(0);

    const arg2 = edgesFromBinding(db, "c", "arg[2]");
    expect(arg2.length, "edge for arg[2]").toBeGreaterThan(0);

    const argN = edgesFromBinding(db, "d", "arg[n]");
    expect(argN.length, "edge for arg[n]").toBeGreaterThan(0);
  });

  // -----------------------------------------------------------------------
  // 4. Multi-step: no direct a→denominator; only a→x, x→y, y→denominator
  // -----------------------------------------------------------------------
  it("emits step-by-step edges only (no alias analysis)", () => {
    ({ db, tmpDir } = openTestDb());
    // Use a function so a and b are parameters and resolvable
    const filePath = writeFixture(
      tmpDir,
      "function run(a, b) { const x = a; const y = x; const r = y / b; return r; }",
    );
    buildSASTForFile(db, filePath);

    // a → x (rhs of const x = a, which is assignment/init)
    const aToX = edgesFromBinding(db, "a", "rhs");
    expect(aToX.length, "a used as rhs in assignment to x").toBeGreaterThan(0);

    // x → y (rhs of const y = x)
    const xToY = edgesFromBinding(db, "x", "rhs");
    expect(xToY.length, "x used as rhs in assignment to y").toBeGreaterThan(0);

    // y → denominator in y / b
    const yDenom = edgesFromBinding(db, "y", "lhs");
    expect(yDenom.length, "y used as lhs in y/b").toBeGreaterThan(0);

    // Verify NO direct edge from 'a' to the denominator slot
    const allEdges = db.select().from(dataFlow).all();
    const aBindings = db.select({ nodeId: nodeBinding.nodeId })
      .from(nodeBinding)
      .where(eq(nodeBinding.name, "a"))
      .all();
    const aIds = new Set(aBindings.map((b) => b.nodeId));
    const aDenomEdges = allEdges.filter((e) => aIds.has(e.fromNode) && e.slot === "denominator");
    expect(aDenomEdges.length, "no direct a→denominator edge (no alias analysis)").toBe(0);
  });

  // -----------------------------------------------------------------------
  // 5. Transitive closure
  // -----------------------------------------------------------------------
  it("transitive closure contains all direct edges", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function run(a, b) { const x = a; const y = x; const r = y / b; return r; }",
    );
    buildSASTForFile(db, filePath);

    const trans = db.select().from(dataFlowTransitive).all();
    expect(trans.length, "transitive rows exist").toBeGreaterThan(0);

    // Every direct edge (to, from) must appear in data_flow_transitive
    const direct = db.select().from(dataFlow).all();
    const transSet = new Set(trans.map((t) => `${t.toNode}\0${t.fromNode}`));
    for (const edge of direct) {
      const key = `${edge.toNode}\0${edge.fromNode}`;
      expect(transSet.has(key), `direct edge (${edge.toNode}, ${edge.fromNode}) in transitive`).toBe(true);
    }

    // Verify chained transitive: x-binding → use-of-x-in-y=x → y-binding → use-of-y-in-r=y
    // The x binding should appear as ancestor of use-of-y (via x-use as intermediate)
    // Since x-use is to_node in edge (x-use ← x-binding) AND x-use is also from_node
    // of (y-use ← x-binding is wrong — y-use ← y-binding).
    // Instead verify: x-binding IS a transitive ancestor of y-use (from_node=x-binding, to_node=y-use)
    // because direct: to=x-use, from=a-binding; and the x-binding is the from of y-use... wait.
    // Actually: transitive links to_node back to ALL ancestors by following from_nodes.
    // x-use has from_node=a-binding (direct). x-use is NOT a to_node for y-use — y-use has from=x-binding.
    // So a-binding is NOT a transitive ancestor of y-use.
    // But x-binding IS a transitive ancestor of use-of-y (directly). This is in direct edges too.
    // Just verify: all direct edges appear in transitive (tested above) and rows > 0.
    expect(direct.length, "at least one direct edge").toBeGreaterThan(0);
  });

  // -----------------------------------------------------------------------
  // 6. Callee slot
  // -----------------------------------------------------------------------
  it("emits callee slot for self-recursive call", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function f(x) { return f(x); }",
    );
    buildSASTForFile(db, filePath);

    const callee = edgesFromBinding(db, "f", "callee");
    expect(callee.length, "edge from fn 'f' with slot 'callee'").toBeGreaterThan(0);

    const arg0 = edgesFromBinding(db, "x", "arg[0]");
    expect(arg0.length, "edge from param 'x' with slot 'arg[0]'").toBeGreaterThan(0);
  });

  // -----------------------------------------------------------------------
  // 7. Assignment fixture (best-effort)
  // -----------------------------------------------------------------------
  it("emits edges for assignment and return", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function h(a) { let x = 0; x = a; return x; }",
    );
    buildSASTForFile(db, filePath);

    // a should appear as rhs of assignment x = a
    const aRhs = edgesFromBinding(db, "a", "rhs");
    expect(aRhs.length, "a used as rhs in x = a").toBeGreaterThan(0);

    // x should appear in return slot (from its binding or from assignment node)
    const xReturn = edgesFromBinding(db, "x", "return");
    expect(xReturn.length, "x used in return statement").toBeGreaterThan(0);
  });

  // -----------------------------------------------------------------------
  // 8. Captures slot
  // -----------------------------------------------------------------------
  it("emits captures edge for closure variable", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "const outer = 42; function inner() { return outer; }",
    );
    buildSASTForFile(db, filePath);

    const captureEdges = edgesFromBinding(db, "outer", "captures");
    expect(captureEdges.length, "captures edge for outer variable").toBeGreaterThan(0);
  });

  // -----------------------------------------------------------------------
  // 9. No cross-file edges (sanity)
  // -----------------------------------------------------------------------
  it("emits at least some edges for any non-trivial file", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(
      tmpDir,
      "function compute(x: number) { return x * 2; }",
    );
    buildSASTForFile(db, filePath);

    const edges = db.select().from(dataFlow).all();
    expect(edges.length, "at least one data-flow edge emitted").toBeGreaterThan(0);
  });
});
