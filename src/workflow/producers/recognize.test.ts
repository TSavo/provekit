/**
 * Recognize stage tests. Mocks recognize() to verify the Stage wrapper
 * mechanics — input canonicalization, output round-trip (including the
 * matched/unmatched discriminated union), cache key shape, registry
 * dispatch. The underlying recognize() function has its own coverage
 * of the SAST/DSL semantics; this file proves the Stage wraps it
 * such that the matched-payload survives the witness round-trip.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";

const { recognizeMock } = vi.hoisted(() => ({ recognizeMock: vi.fn() }));
vi.mock("../../fix/stages/recognize.js", () => ({ recognize: recognizeMock }));

import type { BugLocus, LibraryPrinciple } from "../../fix/types.js";
import type { RecognizeResult } from "../../fix/stages/recognize.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeRecognizeStage,
  RECOGNIZE_CAPABILITY,
  type RecognizeStageInput,
} from "./recognize.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "recognize-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-recognize-test-v1" };

const fakeLocus: BugLocus = {
  file: "src/math.ts",
  line: 1,
  confidence: 1.0,
  primaryNode: "node-1",
  containingFunction: "node-1",
  relatedFunctions: [],
  dataFlowAncestors: [],
  dataFlowDescendants: [],
  dominanceRegion: [],
  postDominanceRegion: [],
};

const fakePrinciple: LibraryPrinciple = {
  id: "division-by-zero",
  bug_class_id: "division-by-zero",
  name: "division-by-zero",
  description: "denominator must not be zero",
  smt2Template: "(declare-const b Int) (assert (= b 0))",
  fixTemplate: { kind: "guard", template: "if (b === 0) return 0;" } as unknown as LibraryPrinciple["fixTemplate"],
  testTemplate: { kind: "regression", template: "expect(divide(1,0)).toBe(0)" } as unknown as LibraryPrinciple["testTemplate"],
  confidence: "high",
};

const fakeMatchedResult: RecognizeResult = {
  matched: true,
  principleId: "division-by-zero",
  bugClassId: "division-by-zero",
  bindings: { denominator: "node-1" },
  principle: fakePrinciple,
  matchId: 42,
  rootMatchNodeId: "node-1",
};

const fakeUnmatchedResult: RecognizeResult = { matched: false };

describe("recognize Stage", () => {
  beforeEach(() => {
    recognizeMock.mockReset();
    recognizeMock.mockResolvedValue(fakeMatchedResult);
  });

  it("runs through WorkflowRunner.runStage and returns the matched RecognizeResult", async () => {
    const db = makeDb();
    const stage = makeRecognizeStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, { locus: fakeLocus });

    expect(result.cacheHit).toBe(false);
    expect(result.output.matched).toBe(true);
    if (result.output.matched) {
      expect(result.output.principleId).toBe("division-by-zero");
      expect(result.output.bindings.denominator).toBe("node-1");
      expect(result.output.matchId).toBe(42);
    }
    expect(recognizeMock).toHaveBeenCalledTimes(1);
    const callArg = recognizeMock.mock.calls[0][0];
    expect(callArg.db).toBe(db);
    expect(callArg.locus).toBe(fakeLocus);
  });

  it("round-trips the unmatched-discriminant on cache hit", async () => {
    const db = makeDb();
    recognizeMock.mockResolvedValue(fakeUnmatchedResult);
    const stage = makeRecognizeStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { locus: fakeLocus });
    const b = await runner.runStage(stage, { locus: fakeLocus });

    expect(a.output.matched).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(b.output.matched).toBe(false);
    expect(recognizeMock).toHaveBeenCalledTimes(1);
  });

  it("caches identical input — second run is a hit, recognize not invoked", async () => {
    const db = makeDb();
    const stage = makeRecognizeStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { locus: fakeLocus });
    const b = await runner.runStage(stage, { locus: fakeLocus });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    // Matched-payload survives round-trip.
    if (b.output.matched && a.output.matched) {
      expect(b.output.principle.id).toBe(a.output.principle.id);
      expect(b.output.bindings).toEqual(a.output.bindings);
    }
    expect(recognizeMock).toHaveBeenCalledTimes(1);
  });

  it("dispatches via the registry as capability 'recognize'", async () => {
    const db = makeDb();
    const stage = makeRecognizeStage({ db });
    const registry = new InMemoryRegistry();
    registry.register(RECOGNIZE_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<RecognizeStageInput, RecognizeResult>(
      RECOGNIZE_CAPABILITY,
      { locus: fakeLocus },
    );

    expect(result.output.matched).toBe(true);
  });
});
