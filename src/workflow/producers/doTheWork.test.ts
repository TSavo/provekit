/**
 * Do-the-work stage tests. Mocks doTheWork() — verifies the Stage
 * cache contract, in particular that:
 *
 *   - the full DoTheWorkResult (patch + test + verdicts) round-trips
 *     through the witness column;
 *   - same content inputs hit the same cache slot regardless of
 *     overlay's runtime path (worktreePath excluded from the hash);
 *   - different baseRef produces different cache slots.
 *
 * The underlying doTheWork() function has its own coverage of the
 * agent + verifier internals; this file proves the Stage wraps it
 * such that the memento captures the unit of work in full.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";

const { doTheWorkMock } = vi.hoisted(() => ({ doTheWorkMock: vi.fn() }));
vi.mock("../../fix/stages/doTheWork.js", () => ({ doTheWork: doTheWorkMock }));

import { StubLLMProvider } from "../../fix/types.js";
import type {
  BugLocus,
  IntentSignal,
  InvariantClaim,
  OverlayHandle,
} from "../../fix/types.js";
import type { DoTheWorkResult } from "../../fix/stages/doTheWork.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeDoTheWorkStage,
  DO_THE_WORK_CAPABILITY,
  type DoTheWorkStageInput,
} from "./doTheWork.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "do-the-work-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-do-the-work-test-v1" };

const fakeSignal: IntentSignal = {
  source: "test",
  rawText: "denominator may be zero",
  summary: "divide may crash on zero denominator",
  failureDescription: "ZeroDivisionError",
  codeReferences: [{ file: "src/math.ts", line: 1 }],
  bugClassHint: "division-by-zero",
};

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

const fakeInvariant: InvariantClaim = {
  principleId: null,
  description: "denominator must not be zero",
  formalExpression: "(declare-const b Int) (assert (= b 0))",
  bindings: [],
  complexity: 3,
  witness: "(model (define-fun b () Int 0))",
  source: "llm",
};

function fakeOverlay(worktreePath: string, baseRef: string): OverlayHandle {
  return {
    worktreePath,
    sastDbPath: join(worktreePath, "sast.db"),
    sastDb: openDb(join(tmpdir(), `unused-${Date.now()}.db`)),
    baseRef,
    modifiedFiles: new Set(),
    closed: false,
  };
}

const fakeResult: DoTheWorkResult = {
  fix: {
    patch: { fileEdits: [{ file: "src/math.ts", oldContent: "a/b", newContent: "b === 0 ? 0 : a/b" }] } as DoTheWorkResult["fix"]["patch"],
    source: "llm",
    llmRationale: "guard the denominator",
    llmConfidence: 0.95,
    invariantHoldsUnderOverlay: true,
    overlayZ3Verdict: "unsat",
    audit: {
      overlayCreated: true,
      patchApplied: true,
      overlayReindexed: true,
      z3RunMs: 42,
      overlayClosed: false,
    },
  },
  test: {
    source: "llm",
    testFilePath: "src/math.regression.test.ts",
    testName: "divide handles zero denominator",
    testCode: "/* test code */",
    witnessInputs: { a: 1, b: 0 },
    passesOnFixedCode: true,
    failsOnOriginalCode: true,
    audit: {
      fixedRunStdout: "",
      fixedRunExitCode: 0,
      originalRunStdout: "",
      originalRunExitCode: 1,
      mutationApplied: true,
      mutationReverted: true,
    },
  },
  rationale: "guard added before division",
  turnsUsed: 3,
};

describe("doTheWork Stage", () => {
  beforeEach(() => {
    doTheWorkMock.mockReset();
    doTheWorkMock.mockResolvedValue(fakeResult);
  });

  it("runs through WorkflowRunner.runStage and returns the full DoTheWorkResult", async () => {
    const db = makeDb();
    const stage = makeDoTheWorkStage({ llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      signal: fakeSignal,
      locus: fakeLocus,
      invariant: fakeInvariant,
      overlay: fakeOverlay("/tmp/overlay-a", "abc123"),
    });

    expect(result.cacheHit).toBe(false);
    expect(result.output.fix.invariantHoldsUnderOverlay).toBe(true);
    expect(result.output.fix.overlayZ3Verdict).toBe("unsat");
    expect(result.output.test.passesOnFixedCode).toBe(true);
    expect(result.output.test.failsOnOriginalCode).toBe(true);
    expect(doTheWorkMock).toHaveBeenCalledTimes(1);
  });

  it("memento captures the unit of work — verdicts round-trip on cache hit", async () => {
    const db = makeDb();
    const stage = makeDoTheWorkStage({ llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      signal: fakeSignal,
      locus: fakeLocus,
      invariant: fakeInvariant,
      overlay: fakeOverlay("/tmp/overlay-a", "abc123"),
    });
    const b = await runner.runStage(stage, {
      signal: fakeSignal,
      locus: fakeLocus,
      invariant: fakeInvariant,
      overlay: fakeOverlay("/tmp/overlay-a", "abc123"),
    });

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    // Verdicts ARE the memento — downstream should read from witness, not re-verify.
    expect(b.output.fix.invariantHoldsUnderOverlay).toBe(true);
    expect(b.output.fix.overlayZ3Verdict).toBe("unsat");
    expect(b.output.test.passesOnFixedCode).toBe(true);
    expect(b.output.test.failsOnOriginalCode).toBe(true);
    expect(doTheWorkMock).toHaveBeenCalledTimes(1);
  });

  it("same baseRef + different worktree paths hit the same cache slot", async () => {
    const db = makeDb();
    const stage = makeDoTheWorkStage({ llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      signal: fakeSignal,
      locus: fakeLocus,
      invariant: fakeInvariant,
      overlay: fakeOverlay("/tmp/overlay-A", "abc123"),
    });
    const b = await runner.runStage(stage, {
      signal: fakeSignal,
      locus: fakeLocus,
      invariant: fakeInvariant,
      overlay: fakeOverlay("/tmp/overlay-B-different-path", "abc123"),
    });

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(doTheWorkMock).toHaveBeenCalledTimes(1);
  });

  it("different baseRef produces different cache slots", async () => {
    const db = makeDb();
    const stage = makeDoTheWorkStage({ llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      signal: fakeSignal,
      locus: fakeLocus,
      invariant: fakeInvariant,
      overlay: fakeOverlay("/tmp/overlay", "abc123"),
    });
    const b = await runner.runStage(stage, {
      signal: fakeSignal,
      locus: fakeLocus,
      invariant: fakeInvariant,
      overlay: fakeOverlay("/tmp/overlay", "def456"),
    });

    expect(a.cid).not.toBe(b.cid);
    expect(b.cacheHit).toBe(false);
    expect(doTheWorkMock).toHaveBeenCalledTimes(2);
  });

  it("dispatches via the registry as capability 'do-the-work'", async () => {
    const db = makeDb();
    const stage = makeDoTheWorkStage({ llm: new StubLLMProvider(new Map()) });
    const registry = new InMemoryRegistry();
    registry.register(DO_THE_WORK_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<DoTheWorkStageInput, DoTheWorkResult>(
      DO_THE_WORK_CAPABILITY,
      {
        signal: fakeSignal,
        locus: fakeLocus,
        invariant: fakeInvariant,
        overlay: fakeOverlay("/tmp/overlay", "abc123"),
      },
    );

    expect(result.output.fix.invariantHoldsUnderOverlay).toBe(true);
  });
});

