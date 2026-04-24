import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { files, nodes, nodeChildren } from "./index.js";
import { eq } from "drizzle-orm";
import { subtreeHash } from "../subtreeHash.js";
import { createHash } from "crypto";

describe("SAST core tables", () => {
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

  function openTestDb() {
    tmpDir = mkdtempSync(join(tmpdir(), "provekit-sast-test-"));
    const dbPath = join(tmpDir, "test.db");
    db = openDb(dbPath);
    migrate(db, { migrationsFolder: "./drizzle" });
    return db;
  }

  it("inserts and retrieves a file row", () => {
    const db = openTestDb();
    const now = Date.now();
    const result = db
      .insert(files)
      .values({ path: "/src/foo.ts", contentHash: "abc123", parsedAt: now })
      .returning()
      .get();
    expect(result.id).toBeGreaterThan(0);
    expect(result.path).toBe("/src/foo.ts");
    expect(result.contentHash).toBe("abc123");
  });

  it("inserts and retrieves a node row with subtree_hash", () => {
    const db = openTestDb();
    const fileRow = db
      .insert(files)
      .values({ path: "/src/foo.ts", contentHash: "abc123", parsedAt: Date.now() })
      .returning()
      .get();

    const text = "function foo() {}";
    const hash = subtreeHash(text);
    const nodeId = createHash("sha256").update(`${fileRow.id}:0:17`).digest("hex");

    db.insert(nodes)
      .values({
        id: nodeId,
        fileId: fileRow.id,
        sourceStart: 0,
        sourceEnd: 17,
        sourceLine: 1,
        sourceCol: 0,
        subtreeHash: hash,
        kind: "SourceFile",
      })
      .run();

    const rows = db.select().from(nodes).where(eq(nodes.id, nodeId)).all();
    expect(rows).toHaveLength(1);
    expect(rows[0].subtreeHash).toBe(hash);
    expect(rows[0].subtreeHash).toBe(
      createHash("sha256").update(text).digest("hex"),
    );
  });

  it("inserts parent + child nodes and retrieves children ordered by child_order", () => {
    const db = openTestDb();
    const fileRow = db
      .insert(files)
      .values({ path: "/src/bar.ts", contentHash: "def456", parsedAt: Date.now() })
      .returning()
      .get();

    const parentId = "parent-id-001";
    const childId1 = "child-id-001";
    const childId2 = "child-id-002";

    db.insert(nodes)
      .values([
        { id: parentId, fileId: fileRow.id, sourceStart: 0, sourceEnd: 100, sourceLine: 1, sourceCol: 0, subtreeHash: "h1", kind: "SourceFile" },
        { id: childId1, fileId: fileRow.id, sourceStart: 10, sourceEnd: 40, sourceLine: 2, sourceCol: 2, subtreeHash: "h2", kind: "FunctionDeclaration" },
        { id: childId2, fileId: fileRow.id, sourceStart: 50, sourceEnd: 90, sourceLine: 5, sourceCol: 2, subtreeHash: "h3", kind: "VariableStatement" },
      ])
      .run();

    db.insert(nodeChildren)
      .values([
        { parentId, childId: childId2, childOrder: 1 },
        { parentId, childId: childId1, childOrder: 0 },
      ])
      .run();

    const edges = db
      .select()
      .from(nodeChildren)
      .where(eq(nodeChildren.parentId, parentId))
      .orderBy(nodeChildren.childOrder)
      .all();

    expect(edges).toHaveLength(2);
    expect(edges[0].childId).toBe(childId1);
    expect(edges[0].childOrder).toBe(0);
    expect(edges[1].childId).toBe(childId2);
    expect(edges[1].childOrder).toBe(1);
  });

  it("cascades delete: deleting a file deletes its nodes", () => {
    const db = openTestDb();
    const fileRow = db
      .insert(files)
      .values({ path: "/src/cascade.ts", contentHash: "ccc", parsedAt: Date.now() })
      .returning()
      .get();

    db.insert(nodes)
      .values({ id: "cascade-node-1", fileId: fileRow.id, sourceStart: 0, sourceEnd: 5, sourceLine: 1, sourceCol: 0, subtreeHash: "hx", kind: "SourceFile" })
      .run();

    db.delete(files).where(eq(files.id, fileRow.id)).run();

    const remaining = db.select().from(nodes).where(eq(nodes.id, "cascade-node-1")).all();
    expect(remaining).toHaveLength(0);
  });

  it("cascades delete: deleting a node deletes its edges in node_children", () => {
    const db = openTestDb();
    const fileRow = db
      .insert(files)
      .values({ path: "/src/edge-cascade.ts", contentHash: "eee", parsedAt: Date.now() })
      .returning()
      .get();

    const parentId = "edge-parent-1";
    const childId = "edge-child-1";

    db.insert(nodes)
      .values([
        { id: parentId, fileId: fileRow.id, sourceStart: 0, sourceEnd: 50, sourceLine: 1, sourceCol: 0, subtreeHash: "hp", kind: "SourceFile" },
        { id: childId, fileId: fileRow.id, sourceStart: 5, sourceEnd: 20, sourceLine: 2, sourceCol: 2, subtreeHash: "hc", kind: "FunctionDeclaration" },
      ])
      .run();

    db.insert(nodeChildren)
      .values({ parentId, childId, childOrder: 0 })
      .run();

    // Delete child node — edge should cascade away
    db.delete(nodes).where(eq(nodes.id, childId)).run();

    const edges = db.select().from(nodeChildren).where(eq(nodeChildren.parentId, parentId)).all();
    expect(edges).toHaveLength(0);
  });
});
