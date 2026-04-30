/**
 * cluster-by-shape stage tests. Pure-data Stage; no DB or filesystem
 * needed for the algorithm itself, but the runner needs a DB for
 * memento storage.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import type { StoredInvariant } from "../../fix/runtime/invariantStore.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeClusterByShapeStage,
  CLUSTER_BY_SHAPE_CAPABILITY,
  type ClusterByShapeResult,
  type ClusterByShapeStageInput,
} from "./clusterByShape.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "cluster-by-shape-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

function makeInv(
  id: string,
  kind: StoredInvariant["smt"]["kind"],
  sorts: string[],
  declarations: string[] = ["(declare-const x Int)"],
): StoredInvariant {
  return {
    id,
    createdAt: "2026-04-29T00:00:00.000Z",
    originatingBug: id,
    smt: { kind, declarations, assertion: "(assert true)" },
    bindings: sorts.map((sort, idx) => ({
      type: "local" as const,
      smt_constant: `x${idx}`,
      source_expr: "expr",
      sort,
      node: {
        filePath: "src/m.ts",
        nodeHash: "h",
        startLine: 1,
        endLine: 1,
      },
    })),
    callsite: {
      filePath: "src/m.ts",
      function: null,
      startLine: 1,
      endLine: 1,
    },
    scope: "callsite",
    regressionTest: null,
    patchSha: null,
    retired: null,
  };
}

const wf = { name: "test-wf", cid: "wf-cluster-test-v1" };

describe("cluster-by-shape Stage", () => {
  it("groups invariants with identical (kind, sorts, declarationCount) fingerprints", async () => {
    const db = makeDb();
    const invariants = [
      makeInv("a", "arithmetic", ["Int", "Int"]),
      makeInv("b", "arithmetic", ["Int", "Int"]),
      makeInv("c", "arithmetic", ["Int", "Bool"]),
      makeInv("d", "cardinality", ["Int"]),
    ];
    const stage = makeClusterByShapeStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, { invariants });

    expect(output.inputCount).toBe(4);
    expect(output.clusters).toHaveLength(3);
    // Largest cluster surfaces first; topCluster mirrors clusters[0].
    expect(output.clusters[0].members.sort()).toEqual(["a", "b"]);
    expect(output.topCluster?.members.sort()).toEqual(["a", "b"]);
    expect(output.topCluster?.shape.kind).toBe("arithmetic");
    expect(output.topCluster?.shape.bindingSorts).toEqual(["Int", "Int"]);
  });

  it("treats sort order as canonical — Int+Bool same fingerprint as Bool+Int", async () => {
    const db = makeDb();
    const invariants = [
      makeInv("a", "arithmetic", ["Int", "Bool"]),
      makeInv("b", "arithmetic", ["Bool", "Int"]),
    ];
    const stage = makeClusterByShapeStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, { invariants });

    expect(output.clusters).toHaveLength(1);
    expect(output.clusters[0].members.sort()).toEqual(["a", "b"]);
  });

  it("skips graph bindings when computing binding sorts", async () => {
    const db = makeDb();
    const inv: StoredInvariant = {
      ...makeInv("a", "arithmetic", []),
      bindings: [
        {
          type: "graph",
          smt_constant: "g",
          relation: "imports_transitively",
          root: { filePath: "src/m.ts" },
          predicate: "no_match",
          predicateArg: "**/forbidden.ts",
        },
      ],
    };
    const stage = makeClusterByShapeStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, { invariants: [inv] });

    expect(output.clusters[0].shape.bindingSorts).toEqual([]);
  });

  it("returns null topCluster on empty input", async () => {
    const db = makeDb();
    const stage = makeClusterByShapeStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, { invariants: [] });

    expect(output.inputCount).toBe(0);
    expect(output.clusters).toEqual([]);
    expect(output.topCluster).toBeNull();
  });

  it("caches identical input — second run is a hit", async () => {
    const db = makeDb();
    const invariants = [
      makeInv("a", "arithmetic", ["Int", "Int"]),
      makeInv("b", "arithmetic", ["Int", "Int"]),
    ];
    const stage = makeClusterByShapeStage();
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { invariants });
    const b = await runner.runStage(stage, { invariants });

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
  });

  it("dispatches via the registry as capability 'cluster-by-shape'", async () => {
    const db = makeDb();
    const stage = makeClusterByShapeStage();
    const registry = new InMemoryRegistry();
    registry.register(CLUSTER_BY_SHAPE_CAPABILITY, stage);
    const runner = new WorkflowRunner(db, wf, registry);

    const result = await runner.request<
      ClusterByShapeStageInput,
      ClusterByShapeResult
    >(CLUSTER_BY_SHAPE_CAPABILITY, {
      invariants: [makeInv("a", "arithmetic", ["Int"])],
    });

    expect(result.output.inputCount).toBe(1);
  });
});
