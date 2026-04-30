/**
 * validate-adversarial stage tests. v1 stub semantics: re-run the
 * cluster's fingerprint against the local corpus and report invariants
 * that match the fingerprint but aren't cluster members. Real cross-
 * codebase validation is a follow-up.
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
  makeValidateAdversarialStage,
  VALIDATE_ADVERSARIAL_CAPABILITY,
  type ValidateAdversarialResult,
  type ValidateAdversarialStageInput,
} from "./validateAdversarial.js";
import type { ShapeCluster } from "./clusterByShape.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "validate-adversarial-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

function makeInv(
  id: string,
  kind: StoredInvariant["smt"]["kind"],
  sorts: string[],
): StoredInvariant {
  return {
    id,
    createdAt: "2026-04-29T00:00:00.000Z",
    originatingBug: id,
    smt: {
      kind,
      declarations: ["(declare-const x Int)"],
      assertion: "(assert true)",
    },
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

const wf = { name: "test-wf", cid: "wf-validate-test-v1" };

const fingerprint = "arithmetic|Int,Int|1";

describe("validate-adversarial Stage", () => {
  it("returns clean when no non-member invariants share the fingerprint", async () => {
    const db = makeDb();
    const cluster: ShapeCluster = {
      fingerprint,
      members: ["a", "b"],
      shape: { kind: "arithmetic", bindingSorts: ["Int", "Int"], declarationCount: 1 },
    };
    const corpus = [
      makeInv("a", "arithmetic", ["Int", "Int"]),
      makeInv("b", "arithmetic", ["Int", "Int"]),
      makeInv("c", "cardinality", ["Int"]),
    ];

    const stage = makeValidateAdversarialStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, { cluster, corpus });

    expect(output.verdict).toBe("clean");
    expect(output.falsePositives).toEqual([]);
    expect(output.validator).toBe("local-fingerprint-only");
  });

  it("flags non-member invariants whose fingerprint collides with the cluster", async () => {
    const db = makeDb();
    const cluster: ShapeCluster = {
      fingerprint,
      members: ["a"],
      shape: { kind: "arithmetic", bindingSorts: ["Int", "Int"], declarationCount: 1 },
    };
    const corpus = [
      makeInv("a", "arithmetic", ["Int", "Int"]),
      makeInv("b", "arithmetic", ["Int", "Int"]), // collides — false positive
      makeInv("c", "cardinality", ["Int"]),
    ];

    const stage = makeValidateAdversarialStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, { cluster, corpus });

    expect(output.verdict).toBe("false-positive");
    expect(output.falsePositives).toEqual(["b"]);
  });

  it("short-circuits on null cluster (empty corpus path)", async () => {
    const db = makeDb();
    const stage = makeValidateAdversarialStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      cluster: null,
      corpus: [],
    });

    expect(output.verdict).toBe("clean");
    expect(output.falsePositives).toEqual([]);
    expect(output.validator).toBe("empty-corpus-short-circuit");
  });

  it("caches identical input — second run is a hit", async () => {
    const db = makeDb();
    const cluster: ShapeCluster = {
      fingerprint,
      members: ["a"],
      shape: { kind: "arithmetic", bindingSorts: ["Int", "Int"], declarationCount: 1 },
    };
    const corpus = [makeInv("a", "arithmetic", ["Int", "Int"])];

    const stage = makeValidateAdversarialStage();
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { cluster, corpus });
    const b = await runner.runStage(stage, { cluster, corpus });

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
  });

  it("dispatches via the registry as capability 'validate-adversarial'", async () => {
    const db = makeDb();
    const stage = makeValidateAdversarialStage();
    const registry = new InMemoryRegistry();
    registry.register(VALIDATE_ADVERSARIAL_CAPABILITY, stage);
    const runner = new WorkflowRunner(db, wf, registry);

    const cluster: ShapeCluster = {
      fingerprint,
      members: ["a"],
      shape: { kind: "arithmetic", bindingSorts: ["Int", "Int"], declarationCount: 1 },
    };
    const result = await runner.request<
      ValidateAdversarialStageInput,
      ValidateAdversarialResult
    >(VALIDATE_ADVERSARIAL_CAPABILITY, {
      cluster,
      corpus: [makeInv("a", "arithmetic", ["Int", "Int"])],
    });

    expect(result.output.verdict).toBe("clean");
  });
});
