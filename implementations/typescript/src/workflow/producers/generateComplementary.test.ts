/**
 * GenerateComplementary stage tests. Mocks generateComplementary() —
 * verifies the Stage cache contract, in particular that:
 *
 *   - the full ComplementaryChange[] (each carrying its Oracle #3
 *     verdict) round-trips through the witness column;
 *   - same content inputs hit the same cache slot regardless of the
 *     overlay's runtime path (worktreePath excluded from the hash);
 *   - different baseRef produces different cache slots.
 *
 * The underlying generateComplementary() function has its own coverage
 * of site-discovery + verifySiteChange semantics; this file proves the
 * Stage wraps it such that the memento captures the unit of work.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";

const { generateComplementaryMock } = vi.hoisted(() => ({
  generateComplementaryMock: vi.fn(),
}));
vi.mock("../../fix/stages/generateComplementary.js", () => ({
  generateComplementary: generateComplementaryMock,
}));

import { StubLLMProvider } from "../../fix/types.js";
import type {
  BugLocus,
  ComplementaryChange,
  FixCandidate,
  InvariantClaim,
  OverlayHandle,
} from "../../fix/types.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeGenerateComplementaryStage,
  GENERATE_COMPLEMENTARY_CAPABILITY,
  type GenerateComplementaryStageInput,
} from "./generateComplementary.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "gen-comp-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-gen-comp-test-v1" };

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

const fakeFix: FixCandidate = {
  patch: {
    fileEdits: [{ file: "src/math.ts", newContent: "export const x = 1;" }],
    description: "guard the denominator",
  },
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
    sastDb: openDb(join(tmpdir(), `unused-${Date.now()}-${Math.random()}.db`)),
    baseRef,
    modifiedFiles: new Set(),
    closed: false,
  };
}

const fakeAccepted: ComplementaryChange[] = [
  {
    kind: "caller_update",
    targetNodeId: "node-2",
    patch: {
      fileEdits: [{ file: "src/caller.ts", newContent: "// updated" }],
      description: "update caller to handle 0",
    },
    rationale: "callers now need to handle the new 0-return",
    verifiedAgainstOverlay: true,
    overlayZ3Verdict: "unsat",
    priority: 1,
    audit: {
      siteKind: "caller_update",
      discoveredVia: "calls_table",
      z3RunMs: 17,
    },
  },
];

describe("generateComplementary Stage", () => {
  beforeEach(() => {
    generateComplementaryMock.mockReset();
    generateComplementaryMock.mockResolvedValue(fakeAccepted);
  });

  it("runs through WorkflowRunner.runStage and returns the accepted ComplementaryChange[]", async () => {
    const db = makeDb();
    const stage = makeGenerateComplementaryStage({
      db,
      llm: new StubLLMProvider(new Map()),
    });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      fix: fakeFix,
      locus: fakeLocus,
      overlay: fakeOverlay("/tmp/overlay-a", "abc123"),
      maxSites: 5,
      invariant: fakeInvariant,
    });

    expect(result.cacheHit).toBe(false);
    expect(result.output).toHaveLength(1);
    expect(result.output[0].kind).toBe("caller_update");
    expect(result.output[0].verifiedAgainstOverlay).toBe(true);
    expect(result.output[0].overlayZ3Verdict).toBe("unsat");
    expect(generateComplementaryMock).toHaveBeenCalledTimes(1);
  });

  it("memento captures the unit of work — verdicts round-trip on cache hit", async () => {
    const db = makeDb();
    const stage = makeGenerateComplementaryStage({
      db,
      llm: new StubLLMProvider(new Map()),
    });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      fix: fakeFix,
      locus: fakeLocus,
      overlay: fakeOverlay("/tmp/overlay-a", "abc123"),
      maxSites: 5,
      invariant: fakeInvariant,
    });
    const b = await runner.runStage(stage, {
      fix: fakeFix,
      locus: fakeLocus,
      overlay: fakeOverlay("/tmp/overlay-a", "abc123"),
      maxSites: 5,
      invariant: fakeInvariant,
    });

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    // Verdicts ARE the memento — downstream should read from witness, not re-verify.
    expect(b.output[0].verifiedAgainstOverlay).toBe(true);
    expect(b.output[0].overlayZ3Verdict).toBe("unsat");
    expect(b.output[0].priority).toBe(1);
    expect(generateComplementaryMock).toHaveBeenCalledTimes(1);
  });

  it("same baseRef + different worktree paths hit the same cache slot", async () => {
    const db = makeDb();
    const stage = makeGenerateComplementaryStage({
      db,
      llm: new StubLLMProvider(new Map()),
    });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      fix: fakeFix,
      locus: fakeLocus,
      overlay: fakeOverlay("/tmp/overlay-A", "abc123"),
      maxSites: 5,
    });
    const b = await runner.runStage(stage, {
      fix: fakeFix,
      locus: fakeLocus,
      overlay: fakeOverlay("/tmp/overlay-B-different-path", "abc123"),
      maxSites: 5,
    });

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(generateComplementaryMock).toHaveBeenCalledTimes(1);
  });

  it("different baseRef produces different cache slots", async () => {
    const db = makeDb();
    const stage = makeGenerateComplementaryStage({
      db,
      llm: new StubLLMProvider(new Map()),
    });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      fix: fakeFix,
      locus: fakeLocus,
      overlay: fakeOverlay("/tmp/overlay", "abc123"),
      maxSites: 5,
    });
    const b = await runner.runStage(stage, {
      fix: fakeFix,
      locus: fakeLocus,
      overlay: fakeOverlay("/tmp/overlay", "def456"),
      maxSites: 5,
    });

    expect(a.cid).not.toBe(b.cid);
    expect(b.cacheHit).toBe(false);
    expect(generateComplementaryMock).toHaveBeenCalledTimes(2);
  });

  it("dispatches via the registry as capability 'generateComplementary'", async () => {
    const db = makeDb();
    const stage = makeGenerateComplementaryStage({
      db,
      llm: new StubLLMProvider(new Map()),
    });
    const registry = new InMemoryRegistry();
    registry.register(GENERATE_COMPLEMENTARY_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<
      GenerateComplementaryStageInput,
      ComplementaryChange[]
    >(GENERATE_COMPLEMENTARY_CAPABILITY, {
      fix: fakeFix,
      locus: fakeLocus,
      overlay: fakeOverlay("/tmp/overlay", "abc123"),
      maxSites: 5,
    });

    expect(result.output).toHaveLength(1);
    expect(result.output[0].verifiedAgainstOverlay).toBe(true);
  });
});
