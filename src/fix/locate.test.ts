/**
 * B2 locate() tests — 8 cases covering match precision, data-flow, dominance,
 * relative filenames, and multi-file isolation.
 */

import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";
import { openDb } from "../db/index.js";
import { buildSASTForFile } from "../sast/builder.js";
import { nodes, files as filesTable } from "../sast/schema/index.js";
import { locate } from "./locate.js";
import type { BugSignal } from "./types.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function openTestDb() {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-locate-test-"));
  const dbPath = join(tmpDir, "test.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: "./drizzle" });
  return { db, tmpDir };
}

function writeFixture(dir: string, filename: string, source: string): string {
  mkdirSync(dir, { recursive: true });
  const filePath = join(dir, filename);
  writeFileSync(filePath, source, "utf8");
  return filePath;
}

function makeSignal(
  refs: BugSignal["codeReferences"],
  overrides?: Partial<Omit<BugSignal, "codeReferences">>,
): BugSignal {
  return {
    source: "test",
    rawText: "test",
    summary: "test bug",
    failureDescription: "test failure",
    codeReferences: refs,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

describe("locate()", () => {
  let tmpDir: string;
  let db: ReturnType<typeof openDb>;

  afterEach(() => {
    try {
      db.$client.close();
    } catch {
      // ignore
    }
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  // -------------------------------------------------------------------------
  // Test 1: Happy path — file + line match
  // -------------------------------------------------------------------------
  it("resolves file+line to primaryNode on that line inside the FunctionDeclaration", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function divide(a: number, b: number) { return a / b; }\n";
    const filePath = writeFixture(tmpDir, "div.ts", source);
    buildSASTForFile(db, filePath);

    const signal = makeSignal([{ file: filePath, line: 1 }]);
    const locus = locate(db, signal);

    expect(locus).not.toBeNull();
    expect(locus!.confidence).toBe(1.0);
    expect(locus!.file).toBe(filePath);
    expect(locus!.line).toBe(1);

    // primaryNode must be a real node on line 1
    const nodeRow = db
      .select({ sourceLine: nodes.sourceLine, kind: nodes.kind })
      .from(nodes)
      .where(eq(nodes.id, locus!.primaryNode))
      .get();
    expect(nodeRow).toBeDefined();
    expect(nodeRow!.sourceLine).toBe(1);

    // containingFunction must be the FunctionDeclaration
    const fnRow = db
      .select({ kind: nodes.kind })
      .from(nodes)
      .where(eq(nodes.id, locus!.containingFunction))
      .get();
    expect(fnRow).toBeDefined();
    expect(fnRow!.kind).toBe("FunctionDeclaration");
  });

  // -------------------------------------------------------------------------
  // Test 2: File-only match (no line)
  // -------------------------------------------------------------------------
  it("file-only reference gives confidence 0.3 and primaryNode = file root", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function divide(a: number, b: number) { return a / b; }\n";
    const filePath = writeFixture(tmpDir, "div2.ts", source);
    const result = buildSASTForFile(db, filePath);

    const signal = makeSignal([{ file: filePath }]);
    const locus = locate(db, signal);

    expect(locus).not.toBeNull();
    expect(locus!.confidence).toBe(0.3);
    expect(locus!.primaryNode).toBe(result.rootNodeId);
    // For a module-level primary, containingFunction is the root itself
    expect(locus!.containingFunction).toBe(result.rootNodeId);
  });

  // -------------------------------------------------------------------------
  // Test 3: No file match → null
  // -------------------------------------------------------------------------
  it("returns null when no file matches the reference", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function divide(a: number, b: number) { return a / b; }\n";
    const filePath = writeFixture(tmpDir, "div3.ts", source);
    buildSASTForFile(db, filePath);

    const signal = makeSignal([{ file: "nonexistent.ts", line: 5 }]);
    const locus = locate(db, signal);

    expect(locus).toBeNull();
  });

  // -------------------------------------------------------------------------
  // Test 4: Multiple refs — best match wins
  // -------------------------------------------------------------------------
  it("picks the exact-line ref over a file-only ref", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function divide(a: number, b: number) { return a / b; }\n";
    const filePath = writeFixture(tmpDir, "div4.ts", source);
    buildSASTForFile(db, filePath);

    const signal = makeSignal([
      { file: filePath },          // file-only
      { file: filePath, line: 1 }, // exact line
    ]);
    const locus = locate(db, signal);

    expect(locus).not.toBeNull();
    expect(locus!.confidence).toBe(1.0);
  });

  // -------------------------------------------------------------------------
  // Test 5: Data-flow neighborhood
  // -------------------------------------------------------------------------
  it("data-flow neighbor arrays contain only valid node IDs (or are empty per bipartite limitation)", () => {
    ({ db, tmpDir } = openTestDb());
    // Put the division on line 2 to isolate it from the function declaration line.
    const source = [
      "function f(a: number, b: number) {",
      "  const q = a / b;",
      "  return q;",
      "}",
    ].join("\n");
    const filePath = writeFixture(tmpDir, "flow.ts", source);
    buildSASTForFile(db, filePath);

    // Point at line 2 (the a/b assignment)
    const signal = makeSignal([{ file: filePath, line: 2 }]);
    const locus = locate(db, signal);

    expect(locus).not.toBeNull();
    expect(locus!.confidence).toBe(1.0);

    // The data-flow ancestors may be empty if the primary node is a non-identifier
    // (BinaryExpression is not a to_node in the bipartite data_flow table — see
    // KNOWN LIMITATION in BugLocus.dataFlowAncestors). If non-empty, all IDs must
    // correspond to real nodes.
    for (const anc of locus!.dataFlowAncestors) {
      const row = db.select({ id: nodes.id }).from(nodes).where(eq(nodes.id, anc)).get();
      expect(row).toBeDefined();
    }
    for (const desc of locus!.dataFlowDescendants) {
      const row = db.select({ id: nodes.id }).from(nodes).where(eq(nodes.id, desc)).get();
      expect(row).toBeDefined();
    }
  });

  // -------------------------------------------------------------------------
  // Test 6: Dominance region
  // -------------------------------------------------------------------------
  it("FunctionDeclaration dominates nodes inside its body when it is the primary", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function divide(a: number, b: number) { return a / b; }\n";
    const filePath = writeFixture(tmpDir, "dom.ts", source);
    buildSASTForFile(db, filePath);

    // Point at line 1 — the non-leaf heuristic should prefer FunctionDeclaration.
    const signal = makeSignal([{ file: filePath, line: 1 }]);
    const locus = locate(db, signal);

    expect(locus).not.toBeNull();

    // Find the FunctionDeclaration node
    const allNodes = db
      .select({ id: nodes.id, kind: nodes.kind })
      .from(nodes)
      .all();
    const fnNode = allNodes.find((n) => n.kind === "FunctionDeclaration");
    expect(fnNode).toBeDefined();

    // All dominanceRegion IDs must be valid nodes
    for (const dominated of locus!.dominanceRegion) {
      const row = db.select({ id: nodes.id }).from(nodes).where(eq(nodes.id, dominated)).get();
      expect(row).toBeDefined();
    }

    // If primary IS the FunctionDeclaration, it must dominate at least its body statements.
    if (locus!.primaryNode === fnNode!.id) {
      expect(locus!.dominanceRegion.length).toBeGreaterThan(0);
    }
  });

  // -------------------------------------------------------------------------
  // Test 7: Relative filename
  // -------------------------------------------------------------------------
  it("loose suffix match resolves 'foo.ts' to an absolute-path file ending in /foo.ts", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "export const x = 1;\n";
    const filePath = writeFixture(tmpDir, "foo.ts", source);
    // filePath is absolute e.g. /tmp/provekit-locate-test-XXX/foo.ts
    buildSASTForFile(db, filePath);

    const signal = makeSignal([{ file: "foo.ts", line: 1 }]);
    const locus = locate(db, signal);

    expect(locus).not.toBeNull();
    expect(locus!.confidence).toBe(1.0);
    // file is preserved from the ref, not the resolved absolute path
    expect(locus!.file).toBe("foo.ts");
  });

  // -------------------------------------------------------------------------
  // Test 8: Multi-file isolation
  // -------------------------------------------------------------------------
  it("locus fields for file A contain no node IDs belonging to file B", () => {
    ({ db, tmpDir } = openTestDb());

    const sourceA = "function fromA() { return 1; }\n";
    const sourceB = "function fromB() { return 2; }\n";
    const filePathA = writeFixture(tmpDir, "fileA.ts", sourceA);
    const filePathB = writeFixture(tmpDir, "fileB.ts", sourceB);

    buildSASTForFile(db, filePathA);
    buildSASTForFile(db, filePathB);

    const fileB = db
      .select({ id: filesTable.id })
      .from(filesTable)
      .where(eq(filesTable.path, filePathB))
      .get();
    expect(fileB).toBeDefined();

    const nodeBIds = new Set(
      db
        .select({ id: nodes.id })
        .from(nodes)
        .where(eq(nodes.fileId, fileB!.id))
        .all()
        .map((n) => n.id),
    );

    const signal = makeSignal([{ file: filePathA, line: 1 }]);
    const locus = locate(db, signal);

    expect(locus).not.toBeNull();

    const allLocusIds = [
      locus!.primaryNode,
      locus!.containingFunction,
      ...locus!.relatedFunctions,
      ...locus!.dataFlowAncestors,
      ...locus!.dataFlowDescendants,
      ...locus!.dominanceRegion,
      ...locus!.postDominanceRegion,
    ];

    for (const id of allLocusIds) {
      expect(nodeBIds.has(id)).toBe(false);
    }
  });
});
