/**
 * Tests for src/fix/runtime/substrate.ts.
 *
 * Coverage:
 *   - openSubstrateDb returns null when .provekit/provekit.db does not exist
 *   - openSubstrateDb opens a read-only Db handle when the file exists
 *   - resolveCallsiteNodeId returns null when the file path is not in the
 *     substrate
 *   - resolveCallsiteNodeId returns null when no node lives at the given line
 *   - resolveCallsiteNodeId returns the smallest-span node when multiple
 *     candidates share the line
 */
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { files, nodes } from "../../sast/schema/nodes.js";
import { openSubstrateDb, resolveCallsiteNodeId } from "./substrate.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

let createdRoots: string[] = [];

afterEach(() => {
  for (const root of createdRoots) {
    try {
      rmSync(root, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  }
  createdRoots = [];
});

function makeProjectRoot(): string {
  const tmp = mkdtempSync(join(tmpdir(), "substrate-test-"));
  createdRoots.push(tmp);
  return tmp;
}

function setUpSubstrate(projectRoot: string) {
  mkdirSync(join(projectRoot, ".provekit"), { recursive: true });
  const db = openDb(join(projectRoot, ".provekit", "provekit.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("openSubstrateDb", () => {
  it("returns null when no .provekit/provekit.db exists", () => {
    const root = makeProjectRoot();
    expect(openSubstrateDb(root)).toBeNull();
  });

  it("returns a Db handle when the file exists", () => {
    const root = makeProjectRoot();
    setUpSubstrate(root);
    const db = openSubstrateDb(root);
    expect(db).not.toBeNull();
    // Sanity: can SELECT off the migrated tables.
    const rows = db!.select().from(files).all();
    expect(rows).toEqual([]);
  });
});

describe("resolveCallsiteNodeId", () => {
  it("returns null when the file path is not in the substrate", () => {
    const root = makeProjectRoot();
    const db = setUpSubstrate(root);
    expect(resolveCallsiteNodeId(db, "src/missing.ts", 1)).toBeNull();
  });

  it("returns null when the file exists but no node lives at the requested line", () => {
    const root = makeProjectRoot();
    const db = setUpSubstrate(root);
    db.insert(files)
      .values({
        path: "src/foo.ts",
        contentHash: "deadbeef",
        parsedAt: 0,
        rootNodeId: "node-root",
      })
      .run();
    const fileRow = db.select().from(files).all()[0];
    db.insert(nodes)
      .values({
        id: "node-1",
        fileId: fileRow.id,
        sourceStart: 0,
        sourceEnd: 10,
        sourceLine: 1,
        sourceCol: 0,
        subtreeHash: "h1",
        kind: "Identifier",
      })
      .run();
    expect(resolveCallsiteNodeId(db, "src/foo.ts", 99)).toBeNull();
  });

  it("returns the smallest-span node when multiple candidates share the line", () => {
    const root = makeProjectRoot();
    const db = setUpSubstrate(root);
    db.insert(files)
      .values({
        path: "src/foo.ts",
        contentHash: "deadbeef",
        parsedAt: 0,
        rootNodeId: "node-root",
      })
      .run();
    const fileRow = db.select().from(files).all()[0];
    // Three nodes at line 5 with different spans; smallest must win.
    db.insert(nodes)
      .values([
        {
          id: "outer",
          fileId: fileRow.id,
          sourceStart: 0,
          sourceEnd: 100,
          sourceLine: 5,
          sourceCol: 0,
          subtreeHash: "h-outer",
          kind: "FunctionDeclaration",
        },
        {
          id: "inner",
          fileId: fileRow.id,
          sourceStart: 40,
          sourceEnd: 50,
          sourceLine: 5,
          sourceCol: 5,
          subtreeHash: "h-inner",
          kind: "Identifier",
        },
        {
          id: "mid",
          fileId: fileRow.id,
          sourceStart: 20,
          sourceEnd: 80,
          sourceLine: 5,
          sourceCol: 2,
          subtreeHash: "h-mid",
          kind: "CallExpression",
        },
      ])
      .run();
    expect(resolveCallsiteNodeId(db, "src/foo.ts", 5)).toBe("inner");
  });

  it("ignores nodes whose sourceLine does not match the requested line", () => {
    const root = makeProjectRoot();
    const db = setUpSubstrate(root);
    db.insert(files)
      .values({
        path: "src/bar.ts",
        contentHash: "x",
        parsedAt: 0,
        rootNodeId: "r",
      })
      .run();
    const fileRow = db.select().from(files).all()[0];
    db.insert(nodes)
      .values([
        {
          id: "line-3-node",
          fileId: fileRow.id,
          sourceStart: 0,
          sourceEnd: 5,
          sourceLine: 3,
          sourceCol: 0,
          subtreeHash: "h3",
          kind: "Identifier",
        },
        {
          id: "line-7-node",
          fileId: fileRow.id,
          sourceStart: 20,
          sourceEnd: 25,
          sourceLine: 7,
          sourceCol: 0,
          subtreeHash: "h7",
          kind: "Identifier",
        },
      ])
      .run();
    expect(resolveCallsiteNodeId(db, "src/bar.ts", 7)).toBe("line-7-node");
    expect(resolveCallsiteNodeId(db, "src/bar.ts", 3)).toBe("line-3-node");
    expect(resolveCallsiteNodeId(db, "src/bar.ts", 5)).toBeNull();
  });
});
