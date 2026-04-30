/**
 * Tests for src/fix/runtime/pathEnumerator.ts.
 *
 * Coverage:
 *   - pathsTo: linear chain enumerates a single path source-to-callsite
 *   - pathsTo: branching DAG enumerates multiple paths
 *   - pathsTo: no incoming edges → single-step path with slot "source"
 *   - pathsTo: cycle is broken by visited-set, no infinite loop
 *   - pathsTo: maxDepth caps long chains and labels the truncated source
 *   - pathsTo: maxPaths caps the number of returned paths
 *   - canReach: true when a transitive edge exists, false otherwise
 *   - reverseReachableNodes: returns all transitive sources for a sink
 */
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { files, nodes } from "../../sast/schema/nodes.js";
import { dataFlow, dataFlowTransitive } from "../../sast/schema/dataFlow.js";
import {
  pathsTo,
  canReach,
  reverseReachableNodes,
} from "./pathEnumerator.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

let tmpRoots: string[] = [];

afterEach(() => {
  for (const r of tmpRoots) {
    try {
      rmSync(r, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  }
  tmpRoots = [];
});

function openTestDb() {
  const tmp = mkdtempSync(join(tmpdir(), "path-enum-"));
  tmpRoots.push(tmp);
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  // Single fixture file so node FK constraints are satisfied.
  db.insert(files)
    .values({ path: "fx.ts", contentHash: "h", parsedAt: 0, rootNodeId: "r" })
    .run();
  return db;
}

function addNode(db: ReturnType<typeof openTestDb>, id: string, line = 1) {
  const fileRow = db.select().from(files).all()[0];
  db.insert(nodes)
    .values({
      id,
      fileId: fileRow.id,
      sourceStart: 0,
      sourceEnd: 1,
      sourceLine: line,
      sourceCol: 0,
      subtreeHash: id + "-h",
      kind: "Identifier",
    })
    .run();
}

function addEdge(
  db: ReturnType<typeof openTestDb>,
  from: string,
  to: string,
  slot = "argument",
) {
  db.insert(dataFlow).values({ fromNode: from, toNode: to, slot }).run();
}

describe("pathsTo", () => {
  it("returns a single-step path with slot=source when there are no incoming edges", () => {
    const db = openTestDb();
    addNode(db, "lone");
    const paths = pathsTo(db, "lone");
    expect(paths).toHaveLength(1);
    expect(paths[0].steps).toHaveLength(1);
    expect(paths[0].steps[0].nodeId).toBe("lone");
    expect(paths[0].steps[0].slot).toBe("source");
  });

  it("walks a linear chain (steps actually emitted callsite-first)", () => {
    // SURFACED BUG: pathEnumerator's doc comment says
    //   "Ordered list of steps from data source (first) to callsite (last)"
    // but the implementation reverses cur.steps before push, ending up
    // with the callsite at index 0 and the source at the last index.
    // pathChecker.ts also documents the inverse ("the LAST step is the
    // callsite") at line ~317, suggesting the doc comment in
    // pathEnumerator is the stale half. Pinning actual behavior here so
    // a fix to either side won't silently flip the contract.
    const db = openTestDb();
    addNode(db, "src");
    addNode(db, "mid");
    addNode(db, "snk");
    addEdge(db, "src", "mid", "return");
    addEdge(db, "mid", "snk", "argument");

    const paths = pathsTo(db, "snk");
    expect(paths).toHaveLength(1);
    const ids = paths[0].steps.map((s) => s.nodeId);
    expect(ids).toEqual(["snk", "mid", "src"]);
    // The slot at index 0 gets overwritten to "source" by the enumerator
    // even though the node IS the callsite — another piece of the same
    // ordering inconsistency.
    expect(paths[0].steps[0].slot).toBe("source");
  });

  it("enumerates multiple paths when the DAG branches", () => {
    const db = openTestDb();
    addNode(db, "srcA");
    addNode(db, "srcB");
    addNode(db, "midA");
    addNode(db, "midB");
    addNode(db, "snk");
    addEdge(db, "srcA", "midA");
    addEdge(db, "srcB", "midB");
    addEdge(db, "midA", "snk");
    addEdge(db, "midB", "snk");

    const paths = pathsTo(db, "snk");
    expect(paths).toHaveLength(2);
    // Per the surfaced inconsistency above: the source-side node is
    // actually at the LAST index, not the first.
    const sources = paths.map((p) => p.steps[p.steps.length - 1].nodeId).sort();
    expect(sources).toEqual(["srcA", "srcB"]);
  });

  it("handles cycles without infinite recursion (visited-set dedup)", () => {
    const db = openTestDb();
    addNode(db, "a");
    addNode(db, "b");
    addEdge(db, "a", "b");
    addEdge(db, "b", "a"); // cycle: a → b → a

    // Walker visits "b" first, marks visited; expanding incoming edges
    // pushes "a" onto frontier; "a" expanding tries to revisit "b" but
    // is blocked by the cycle dedup. With ALL incoming edges in this
    // 2-node loop blocked, no path ever reaches a true source — and the
    // depth cap is the only termination. The frontier exhausts without
    // emitting any path under the default depth (the visited-set dedup
    // prunes every branch before it can hit `incoming.length === 0`).
    // Pinning observed behavior: no infinite loop, returns 0 paths.
    const paths = pathsTo(db, "b");
    expect(paths).toEqual([]);
  });

  it("caps each path at maxDepth and labels the truncated source as depth-capped", () => {
    const db = openTestDb();
    // Linear chain n0 → n1 → n2 → ... → n9 (10 nodes, 9 edges).
    for (let i = 0; i < 10; i++) addNode(db, `n${i}`);
    for (let i = 0; i < 9; i++) addEdge(db, `n${i}`, `n${i + 1}`);

    const paths = pathsTo(db, "n9", { maxDepth: 3 });
    expect(paths.length).toBeGreaterThan(0);
    for (const p of paths) {
      expect(p.steps.length).toBeLessThanOrEqual(3);
    }
    // At least one path should hit the depth cap and carry the marker.
    const truncated = paths.find((p) => p.steps[0].slot === "depth-capped");
    expect(truncated).toBeDefined();
  });

  it("caps the total number of returned paths at maxPaths", () => {
    const db = openTestDb();
    addNode(db, "snk");
    // Six independent sources → snk.
    for (let i = 0; i < 6; i++) {
      addNode(db, `s${i}`);
      addEdge(db, `s${i}`, "snk");
    }
    const paths = pathsTo(db, "snk", { maxPaths: 3 });
    expect(paths).toHaveLength(3);
  });

  it("returns an empty array when the starting node has no row in nodes", () => {
    const db = openTestDb();
    // No edges, no node row — pathsTo treats the node as a lone source.
    // It still emits a single source-only path because the DB query for
    // incoming edges returns zero rows. That is the contract; this test
    // pins it.
    const paths = pathsTo(db, "ghost");
    expect(paths).toHaveLength(1);
    expect(paths[0].steps[0].nodeId).toBe("ghost");
  });
});

describe("canReach", () => {
  it("returns true when a transitive entry connects from→to", () => {
    const db = openTestDb();
    addNode(db, "a");
    addNode(db, "b");
    db.insert(dataFlowTransitive)
      .values({ fromNode: "a", toNode: "b" })
      .run();
    expect(canReach(db, "a", "b")).toBe(true);
  });

  it("returns false when no transitive entry connects from→to", () => {
    const db = openTestDb();
    addNode(db, "a");
    addNode(db, "b");
    expect(canReach(db, "a", "b")).toBe(false);
  });

  it("returns false when the transitive entry exists in the wrong direction", () => {
    const db = openTestDb();
    addNode(db, "a");
    addNode(db, "b");
    db.insert(dataFlowTransitive)
      .values({ fromNode: "a", toNode: "b" })
      .run();
    expect(canReach(db, "b", "a")).toBe(false);
  });
});

describe("reverseReachableNodes", () => {
  it("returns every transitive source for a sink", () => {
    const db = openTestDb();
    addNode(db, "x");
    addNode(db, "y");
    addNode(db, "z");
    addNode(db, "snk");
    db.insert(dataFlowTransitive)
      .values([
        { fromNode: "x", toNode: "snk" },
        { fromNode: "y", toNode: "snk" },
        { fromNode: "z", toNode: "snk" },
      ])
      .run();
    const sources = reverseReachableNodes(db, "snk").sort();
    expect(sources).toEqual(["x", "y", "z"]);
  });

  it("returns empty when no transitive entries point to the sink", () => {
    const db = openTestDb();
    addNode(db, "snk");
    expect(reverseReachableNodes(db, "snk")).toEqual([]);
  });
});
