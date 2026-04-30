/**
 * collect-invariants stage tests. Exercises the Stage wrapper end-to-end
 * against a real `.provekit/invariants/` directory built via
 * writeInvariant() — no mocking, since the producer is pure I/O over the
 * existing invariantStore module.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import {
  writeInvariant,
  type StoredInvariant,
} from "../../fix/runtime/invariantStore.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeCollectInvariantsStage,
  COLLECT_INVARIANTS_CAPABILITY,
  type CollectInvariantsResult,
  type CollectInvariantsStageInput,
} from "./collectInvariants.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeProjectAndDb() {
  const projectRoot = mkdtempSync(join(tmpdir(), "collect-invariants-"));
  mkdirSync(join(projectRoot, ".provekit"), { recursive: true });
  const db = openDb(join(projectRoot, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return { projectRoot, db };
}

function makeInvariant(id: string, kind: StoredInvariant["smt"]["kind"]): StoredInvariant {
  return {
    id,
    createdAt: "2026-04-29T00:00:00.000Z",
    originatingBug: `bug-${id}`,
    smt: {
      kind,
      declarations: ["(declare-const x Int)"],
      assertion: "(assert (not (= x 0)))",
    },
    bindings: [
      {
        type: "local",
        smt_constant: "x",
        source_expr: "denominator",
        sort: "Int",
        node: {
          filePath: "src/math.ts",
          nodeHash: "abc",
          startLine: 1,
          endLine: 1,
        },
      },
    ],
    callsite: { filePath: "src/math.ts", function: null, startLine: 1, endLine: 1 },
    scope: "callsite",
    regressionTest: null,
    patchSha: null,
    retired: null,
  };
}

const wf = { name: "test-wf", cid: "wf-collect-test-v1" };

describe("collect-invariants Stage", () => {
  it("returns every invariant when propertyHashes is omitted", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    writeInvariant(projectRoot, makeInvariant("aa", "arithmetic"));
    writeInvariant(projectRoot, makeInvariant("bb", "cardinality"));

    const stage = makeCollectInvariantsStage();
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, { projectRoot });

    expect(result.cacheHit).toBe(false);
    expect(result.output.matchCount).toBe(2);
    expect(result.output.invariants.map((i) => i.id).sort()).toEqual(["aa", "bb"]);
  });

  it("filters by propertyHashes whitelist", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    writeInvariant(projectRoot, makeInvariant("aa", "arithmetic"));
    writeInvariant(projectRoot, makeInvariant("bb", "cardinality"));
    writeInvariant(projectRoot, makeInvariant("cc", "order"));

    const stage = makeCollectInvariantsStage();
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      projectRoot,
      propertyHashes: ["aa", "cc"],
    });

    expect(result.output.matchCount).toBe(2);
    expect(result.output.invariants.map((i) => i.id).sort()).toEqual(["aa", "cc"]);
  });

  it("returns empty when no invariants directory exists", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const stage = makeCollectInvariantsStage();
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, { projectRoot });

    expect(result.output.matchCount).toBe(0);
    expect(result.output.invariants).toEqual([]);
  });

  it("caches identical input — second run is a hit", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    writeInvariant(projectRoot, makeInvariant("aa", "arithmetic"));

    const stage = makeCollectInvariantsStage();
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { projectRoot });
    const b = await runner.runStage(stage, { projectRoot });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(b.output.matchCount).toBe(1);
  });

  it("dispatches via the registry as capability 'collect-invariants'", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    writeInvariant(projectRoot, makeInvariant("aa", "arithmetic"));

    const stage = makeCollectInvariantsStage();
    const registry = new InMemoryRegistry();
    registry.register(COLLECT_INVARIANTS_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<
      CollectInvariantsStageInput,
      CollectInvariantsResult
    >(COLLECT_INVARIANTS_CAPABILITY, { projectRoot });

    expect(result.output.matchCount).toBe(1);
  });

  it("canonicalizes propertyHashes order in the input hash", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    writeInvariant(projectRoot, makeInvariant("aa", "arithmetic"));
    writeInvariant(projectRoot, makeInvariant("bb", "cardinality"));

    const stage = makeCollectInvariantsStage();
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      projectRoot,
      propertyHashes: ["aa", "bb"],
    });
    const b = await runner.runStage(stage, {
      projectRoot,
      propertyHashes: ["bb", "aa"],
    });

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
  });
});
