/**
 * A6: Incremental re-index tests.
 *
 * Tests verify buildSASTForFile short-circuit behaviour and reindexFile
 * force-rebuild semantics.
 */

import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { buildSASTForFile, reindexFile } from "./builder.js";
import { files, nodes, nodeArithmetic, dataFlow } from "./schema/index.js";
import { eq } from "drizzle-orm";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function openTestDb() {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-incremental-test-"));
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

type Db = ReturnType<typeof openDb>;

function nodeCountForFile(db: Db, fileId: number): number {
  return db.select().from(nodes).where(eq(nodes.fileId, fileId)).all().length;
}

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

describe("incremental re-index (A6)", () => {
  let tmpDir: string;
  let db: Db;

  afterEach(() => {
    try { db.$client.close(); } catch { /* ignore */ }
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  // -----------------------------------------------------------------------
  // 1. Short-circuit verified: second build returns rebuilt: false, no dup rows
  // -----------------------------------------------------------------------
  it("second buildSASTForFile call returns rebuilt: false and does not duplicate nodes", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(tmpDir, "a.ts", "const x = 1;");

    const first = buildSASTForFile(db, filePath);
    expect(first.rebuilt).toBe(true);
    const countAfterFirst = nodeCountForFile(db, first.fileId);

    const second = buildSASTForFile(db, filePath);
    expect(second.rebuilt).toBe(false);

    // fileId must be the same (no new row created)
    expect(second.fileId).toBe(first.fileId);

    // Node count must be unchanged
    const countAfterSecond = nodeCountForFile(db, second.fileId);
    expect(countAfterSecond).toBe(countAfterFirst);
  });

  // -----------------------------------------------------------------------
  // 2. Cache hit returns correct rootNodeId
  // -----------------------------------------------------------------------
  it("cache hit returns the same rootNodeId as the initial build", () => {
    ({ db, tmpDir } = openTestDb());
    const filePath = writeFixture(tmpDir, "b.ts", "export function greet() { return 'hi'; }");

    const first = buildSASTForFile(db, filePath);
    const second = buildSASTForFile(db, filePath);

    expect(second.rebuilt).toBe(false);
    expect(second.rootNodeId).toBe(first.rootNodeId);
  });

  // -----------------------------------------------------------------------
  // 3. reindexFile forces rebuild even when content unchanged
  // -----------------------------------------------------------------------
  it("reindexFile returns rebuilt: true and replaces the file row even when content is unchanged", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function add(a: number, b: number) { return a + b; }";
    const filePath = writeFixture(tmpDir, "c.ts", source);

    const first = buildSASTForFile(db, filePath);
    const firstFileId = first.fileId;
    const firstNodeCount = nodeCountForFile(db, firstFileId);

    const second = reindexFile(db, filePath);
    expect(second.rebuilt).toBe(true);

    // Old file row must be gone
    const oldFileRows = db.select().from(files).where(eq(files.id, firstFileId)).all();
    expect(oldFileRows).toHaveLength(0);

    // Old nodes must be gone (FK cascade)
    const oldNodeCount = nodeCountForFile(db, firstFileId);
    expect(oldNodeCount).toBe(0);

    // New file row must be present
    const newFileRows = db.select().from(files).where(eq(files.path, filePath)).all();
    expect(newFileRows).toHaveLength(1);

    // New fileId must differ (auto-increment never reuses)
    expect(second.fileId).not.toBe(firstFileId);

    // Same source => same tree shape => same node count
    expect(second.nodeCount).toBe(firstNodeCount);
  });

  // -----------------------------------------------------------------------
  // 4. reindexFile after on-disk edit: new node count reflects new source
  // -----------------------------------------------------------------------
  it("reindexFile after on-disk edit reflects the new source and drops old nodes", () => {
    ({ db, tmpDir } = openTestDb());
    const source1 = "const a = 1;";
    const source2 = "const a = 1;\nconst b = 2;\nconst c = 3;\nconst d = 4;";
    const filePath = writeFixture(tmpDir, "d.ts", source1);

    const first = buildSASTForFile(db, filePath);
    const firstFileId = first.fileId;

    // Modify file on disk
    writeFileSync(filePath, source2, "utf8");

    const second = reindexFile(db, filePath);
    expect(second.rebuilt).toBe(true);
    expect(second.nodeCount).toBeGreaterThan(first.nodeCount);

    // Old nodes must be gone
    expect(nodeCountForFile(db, firstFileId)).toBe(0);

    // Only one files row for this path
    const fileRows = db.select().from(files).where(eq(files.path, filePath)).all();
    expect(fileRows).toHaveLength(1);
    expect(fileRows[0].id).toBe(second.fileId);
  });

  // -----------------------------------------------------------------------
  // 5. Capabilities and data-flow re-populated after reindexFile
  // -----------------------------------------------------------------------
  it("reindexFile re-populates node_arithmetic and data_flow scoped to the new fileId", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function f(a, b) { const q = a / b; return q; }";
    const filePath = writeFixture(tmpDir, "e.ts", source);

    const first = buildSASTForFile(db, filePath);
    const firstFileId = first.fileId;

    const second = reindexFile(db, filePath);
    const newFileId = second.fileId;

    // node_arithmetic rows scoped to new file (join on nodes.fileId)
    const arithRows = db
      .select({ nodeId: nodeArithmetic.nodeId })
      .from(nodeArithmetic)
      .innerJoin(nodes, eq(nodeArithmetic.nodeId, nodes.id))
      .where(eq(nodes.fileId, newFileId))
      .all();
    expect(arithRows.length, "node_arithmetic rows for new file").toBeGreaterThan(0);

    // node_arithmetic rows scoped to OLD file must be zero (FK cascade)
    const oldArithRows = db
      .select({ nodeId: nodeArithmetic.nodeId })
      .from(nodeArithmetic)
      .innerJoin(nodes, eq(nodeArithmetic.nodeId, nodes.id))
      .where(eq(nodes.fileId, firstFileId))
      .all();
    expect(oldArithRows.length, "no node_arithmetic rows for old (deleted) file").toBe(0);

    // data_flow rows scoped to new file
    const dfRows = db
      .select({ toNode: dataFlow.toNode })
      .from(dataFlow)
      .innerJoin(nodes, eq(dataFlow.toNode, nodes.id))
      .where(eq(nodes.fileId, newFileId))
      .all();
    expect(dfRows.length, "data_flow rows for new file").toBeGreaterThan(0);
  });

  // -----------------------------------------------------------------------
  // 6. Multi-file isolation: reindexFile(A) does not disturb file B
  // -----------------------------------------------------------------------
  it("reindexFile on file A leaves file B's rows completely intact", () => {
    ({ db, tmpDir } = openTestDb());

    const filePathA = writeFixture(tmpDir, "fileA.ts", "const a = 42;");
    const filePathB = writeFixture(tmpDir, "fileB.ts", "export function helper(x: number) { return x * 2; }");

    buildSASTForFile(db, filePathA);
    const firstB = buildSASTForFile(db, filePathB);
    const bFileId = firstB.fileId;
    const bNodeCountBefore = nodeCountForFile(db, bFileId);

    reindexFile(db, filePathA);

    // File B's row must still exist with the same id
    const bRow = db.select().from(files).where(eq(files.id, bFileId)).get();
    expect(bRow, "file B row still present").toBeDefined();

    // File B's node count must be unchanged
    const bNodeCountAfter = nodeCountForFile(db, bFileId);
    expect(bNodeCountAfter).toBe(bNodeCountBefore);
  });
});
