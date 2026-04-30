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
import { openSubstrateDb, resolveCallsiteNodeId, findFunctionLineByHash } from "./substrate.js";

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

describe("self-healing binding (functionHash + functionOffset recovery)", () => {
  function setupFnSubstrate(): { db: ReturnType<typeof openDb>; fnLine: number; offset: number } {
    const root = makeProjectRoot();
    const db = setUpSubstrate(root);
    db.insert(files).values({ path: "src/x.ts", contentHash: "h", parsedAt: 0, rootNodeId: "r" }).run();
    const fileRow = db.select().from(files).all()[0];
    // Function declaration whose body shifted from line 14 (mint time) to line
    // 17 (current). Both rows live in the substrate; the resolver must use
    // functionHash to find the function's current line then add the recorded
    // offset to recompute the callsite's current line.
    db.insert(nodes)
      .values([
        {
          id: "fn-current",
          fileId: fileRow.id,
          sourceStart: 200,
          sourceEnd: 400,
          sourceLine: 17,
          sourceCol: 0,
          subtreeHash: "fn-content-hash-abc",
          kind: "FunctionDeclaration",
        },
        {
          id: "stmt-at-19",
          fileId: fileRow.id,
          sourceStart: 250,
          sourceEnd: 270,
          sourceLine: 19,
          sourceCol: 2,
          subtreeHash: "stmt-h",
          kind: "ExpressionStatement",
        },
      ])
      .run();
    // mint time: function was at line 14, callsite at line 16 → offset 2.
    // current: function moved to line 17, so callsite is at 17 + 2 = 19.
    return { db, fnLine: 14, offset: 2 };
  }

  it("findFunctionLineByHash returns the function's current line", () => {
    const { db } = setupFnSubstrate();
    expect(findFunctionLineByHash(db, "src/x.ts", "fn-content-hash-abc")).toBe(17);
    expect(findFunctionLineByHash(db, "src/x.ts", "no-such-hash")).toBeNull();
    expect(findFunctionLineByHash(db, "src/missing.ts", "any")).toBeNull();
  });

  it("case 1: direct line still resolves; recovery path is not used", () => {
    const { db } = setupFnSubstrate();
    // The recorded line 19 still has a node directly. Returns immediately.
    expect(
      resolveCallsiteNodeId(db, "src/x.ts", 19, {
        functionHash: "fn-content-hash-abc",
        functionOffset: 2,
      }),
    ).toBe("stmt-at-19");
  });

  it("case 2: recorded line missed; functionHash + offset recover the new line", () => {
    const { db } = setupFnSubstrate();
    // Recorded line 16 (mint time) doesn't hit anything any more, but the
    // function moved to line 17 and the recorded offset 2 places the callsite
    // at line 19, which DOES exist.
    expect(
      resolveCallsiteNodeId(db, "src/x.ts", 16, {
        functionHash: "fn-content-hash-abc",
        functionOffset: 2,
      }),
    ).toBe("stmt-at-19");
  });

  it("case 3: functionHash present but no node has that hash → null (semantic decay)", () => {
    const { db } = setupFnSubstrate();
    // Recorded line missed AND function hash isn't in the substrate any more.
    // The function got edited; semantic decay. Resolver returns null; the
    // caller routes this to the LLM-driven re-evaluation workflow.
    expect(
      resolveCallsiteNodeId(db, "src/x.ts", 16, {
        functionHash: "fn-content-hash-was-edited",
        functionOffset: 2,
      }),
    ).toBeNull();
  });

  it("case 4: legacy invariant with no recovery hints, line missed → null (decayed)", () => {
    const { db } = setupFnSubstrate();
    // No recovery hints. Pure line lookup. Misses report null exactly as
    // they did before this feature landed.
    expect(resolveCallsiteNodeId(db, "src/x.ts", 16)).toBeNull();
  });
});
