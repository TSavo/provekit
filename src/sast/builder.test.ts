import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { createHash } from "crypto";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { Project } from "ts-morph";
import type { Node } from "ts-morph";
import { openDb } from "../db/index.js";
import { files, nodes, nodeChildren } from "./schema/index.js";
import { eq } from "drizzle-orm";
import { buildSASTForFile } from "./builder.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function countConcreteNodes(node: Node): number {
  const children = node.getChildren();
  return 1 + children.reduce((sum, c) => sum + countConcreteNodes(c), 0);
}

function openTestDb() {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-builder-test-"));
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

function sha256hex(str: string): string {
  return createHash("sha256").update(str).digest("hex");
}

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

describe("buildSASTForFile", () => {
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
  // Test 1: node count matches a programmatic ts-morph walk
  // -------------------------------------------------------------------------
  it("inserts every concrete node (count matches programmatic walk)", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "function foo(x: number): number { return x + 1; }";
    const filePath = writeFixture(tmpDir, "foo.ts", source);

    const project = new Project({ useInMemoryFileSystem: true });
    const sf = project.createSourceFile(filePath, source);
    const expectedCount = countConcreteNodes(sf);

    const result = buildSASTForFile(db, filePath);

    expect(result.rebuilt).toBe(true);
    expect(result.nodeCount).toBe(expectedCount);
    expect(result.fileId).toBeGreaterThan(0);
    expect(result.rootNodeId).toBeTruthy();
  });

  // -------------------------------------------------------------------------
  // Test 2: root node subtree_hash == sha256(full source text)
  // -------------------------------------------------------------------------
  it("root node subtree_hash equals sha256 of the full source text", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "const answer = 42;\n";
    const filePath = writeFixture(tmpDir, "answer.ts", source);

    const result = buildSASTForFile(db, filePath);

    const rootRow = db.select().from(nodes).where(eq(nodes.id, result.rootNodeId)).get();
    expect(rootRow).toBeDefined();
    expect(rootRow!.subtreeHash).toBe(sha256hex(source));
  });

  // -------------------------------------------------------------------------
  // Test 3: root's children concatenated (via sourceStart/sourceEnd) reproduce
  //         the full source text, confirming child_order is correctly assigned
  // -------------------------------------------------------------------------
  it("root children in child_order reproduce the full source text", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "const a = 1;\nconst b = 2;\n";
    const filePath = writeFixture(tmpDir, "two.ts", source);

    const result = buildSASTForFile(db, filePath);

    // Get root's children ordered by child_order
    const childEdges = db
      .select()
      .from(nodeChildren)
      .where(eq(nodeChildren.parentId, result.rootNodeId))
      .orderBy(nodeChildren.childOrder)
      .all();

    // For each child get the node row and slice source
    const childRows = childEdges.map((edge) =>
      db.select().from(nodes).where(eq(nodes.id, edge.childId)).get()!,
    );

    const reconstructed = childRows.map((n) => source.slice(n.sourceStart, n.sourceEnd)).join("");
    expect(reconstructed).toBe(source);
  });

  // -------------------------------------------------------------------------
  // Test 4: idempotency — second call returns rebuilt: false, row count unchanged
  // -------------------------------------------------------------------------
  it("second call with same content returns rebuilt: false and does not change node count", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "export const x = 99;";
    const filePath = writeFixture(tmpDir, "idem.ts", source);

    const first = buildSASTForFile(db, filePath);
    const nodesBefore = db.select().from(nodes).where(eq(nodes.fileId, first.fileId)).all().length;

    const second = buildSASTForFile(db, filePath);
    const nodesAfter = db.select().from(nodes).where(eq(nodes.fileId, second.fileId)).all().length;

    expect(second.rebuilt).toBe(false);
    expect(nodesAfter).toBe(nodesBefore);
  });

  // -------------------------------------------------------------------------
  // Test 5: content-change invalidation — rebuild with new source
  // -------------------------------------------------------------------------
  it("after content change, rebuild inserts new nodes and returns rebuilt: true", () => {
    ({ db, tmpDir } = openTestDb());
    const source1 = "const a = 1;";
    const source2 = "const a = 1;\nconst b = 2;\nconst c = 3;";
    const filePath = writeFixture(tmpDir, "change.ts", source1);

    const first = buildSASTForFile(db, filePath);
    expect(first.rebuilt).toBe(true);

    // Overwrite the file with different content
    writeFileSync(filePath, source2, "utf8");

    const second = buildSASTForFile(db, filePath);
    expect(second.rebuilt).toBe(true);
    expect(second.nodeCount).toBeGreaterThan(first.nodeCount);

    // Old file row should be gone; only one files row for this path
    const fileRows = db.select().from(files).where(eq(files.path, filePath)).all();
    expect(fileRows).toHaveLength(1);
    expect(fileRows[0].contentHash).toBe(sha256hex(source2));
  });

  // -------------------------------------------------------------------------
  // Test 6: unicode — non-ASCII identifiers don't crash, hash is correct
  // -------------------------------------------------------------------------
  it("handles unicode identifiers without crashing and root hash matches", () => {
    ({ db, tmpDir } = openTestDb());
    const source = "const 日本語 = 1;\n"; // 日本語
    const filePath = writeFixture(tmpDir, "unicode.ts", source);

    const result = buildSASTForFile(db, filePath);
    expect(result.rebuilt).toBe(true);
    expect(result.nodeCount).toBeGreaterThan(0);

    const rootRow = db.select().from(nodes).where(eq(nodes.id, result.rootNodeId)).get();
    expect(rootRow!.subtreeHash).toBe(sha256hex(source));
  });
});
