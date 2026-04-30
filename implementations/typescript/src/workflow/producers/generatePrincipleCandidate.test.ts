/**
 * GeneratePrincipleCandidate stage tests. Mocks
 * generatePrincipleCandidate() — verifies the Stage cache contract,
 * in particular that:
 *
 *   - the PrincipleCandidate[] (canonical + alternatives) round-trips
 *     through the witness column on cache hit;
 *   - the matched / unmatched recognized branch produces different
 *     cache slots (the side-effect AND the output differ);
 *   - empty array (recognized-matched short-circuit, non-codifiable,
 *     or substrate failure) round-trips correctly.
 *
 * The underlying generatePrincipleCandidate() function has its own
 * coverage of the substrate / capability-gap / provenance-append
 * semantics; this file proves the Stage wraps it such that the
 * memento captures the unit of work.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";

const { genPrincipleMock } = vi.hoisted(() => ({ genPrincipleMock: vi.fn() }));
vi.mock("../../fix/stages/generatePrincipleCandidate.js", () => ({
  generatePrincipleCandidate: genPrincipleMock,
}));

import { StubLLMProvider } from "../../fix/types.js";
import type {
  BugSignal,
  FixCandidate,
  InvariantClaim,
  LibraryPrinciple,
  PrincipleCandidate,
} from "../../fix/types.js";
import type { RecognizeResult } from "../../fix/stages/recognize.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeGeneratePrincipleCandidateStage,
  GENERATE_PRINCIPLE_CANDIDATE_CAPABILITY,
  type GeneratePrincipleCandidateStageInput,
} from "./generatePrincipleCandidate.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "gen-principle-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-gen-principle-test-v1" };

const fakeSignal: BugSignal = {
  source: "test",
  rawText: "denominator may be zero",
  summary: "divide may crash on zero denominator",
  failureDescription: "ZeroDivisionError",
  codeReferences: [{ file: "src/math.ts", line: 1 }],
  bugClassHint: "division-by-zero",
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

const fakeFix: FixCandidate = {
  patch: {
    fileEdits: [{ file: "src/math.ts", newContent: "guard" }],
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

const fakePrincipleLib: LibraryPrinciple = {
  id: "division-by-zero",
  bug_class_id: "division-by-zero",
  name: "division-by-zero",
};

const fakeCandidates: PrincipleCandidate[] = [
  {
    kind: "principle",
    name: "division-by-zero",
    bugClassId: "division-by-zero",
    dslSource: "// dsl",
    smtTemplate: "(declare-const b Int) (assert (= b 0))",
    teachingExample: { domain: "math", explanation: "div by zero", smt2: "(declare-const b Int)" },
    adversarialValidation: [
      { validatorModel: "stub", result: "pass", evidence: "ok" },
    ],
    latentSiteMatches: [{ nodeId: "node-1", file: "src/math.ts", line: 1 }],
  },
];

const matchedRecognize: RecognizeResult = {
  matched: true,
  principleId: "division-by-zero",
  bugClassId: "division-by-zero",
  bindings: { denominator: "node-1" },
  principle: fakePrincipleLib,
  matchId: 42,
  rootMatchNodeId: "node-1",
};

describe("generatePrincipleCandidate Stage", () => {
  beforeEach(() => {
    genPrincipleMock.mockReset();
    genPrincipleMock.mockResolvedValue(fakeCandidates);
  });

  it("runs through WorkflowRunner.runStage and returns the PrincipleCandidate[]", async () => {
    const db = makeDb();
    const stage = makeGeneratePrincipleCandidateStage({
      db,
      llm: new StubLLMProvider(new Map()),
    });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      signal: fakeSignal,
      invariant: fakeInvariant,
      fixCandidate: fakeFix,
    });

    expect(result.cacheHit).toBe(false);
    expect(result.output).toHaveLength(1);
    expect(result.output[0].kind).toBe("principle");
    expect(result.output[0].name).toBe("division-by-zero");
    expect(genPrincipleMock).toHaveBeenCalledTimes(1);
    const callArg = genPrincipleMock.mock.calls[0][0];
    expect(callArg.signal).toBe(fakeSignal);
    expect(callArg.invariant).toBe(fakeInvariant);
    expect(callArg.fixCandidate).toBe(fakeFix);
    expect(callArg.db).toBe(db);
  });

  it("caches identical input — second run is a hit, generator not invoked again", async () => {
    const db = makeDb();
    const stage = makeGeneratePrincipleCandidateStage({
      db,
      llm: new StubLLMProvider(new Map()),
    });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      signal: fakeSignal,
      invariant: fakeInvariant,
      fixCandidate: fakeFix,
    });
    const b = await runner.runStage(stage, {
      signal: fakeSignal,
      invariant: fakeInvariant,
      fixCandidate: fakeFix,
    });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(b.output[0].name).toBe(a.output[0].name);
    expect(b.output[0].latentSiteMatches).toEqual(a.output[0].latentSiteMatches);
    expect(genPrincipleMock).toHaveBeenCalledTimes(1);
  });

  it("recognized=matched is a different cache slot than unmatched/absent", async () => {
    const db = makeDb();
    const stage = makeGeneratePrincipleCandidateStage({
      db,
      llm: new StubLLMProvider(new Map()),
    });
    const runner = new WorkflowRunner(db, wf);

    // C6m: recognized matched → impl returns []. Different slot than the unmatched branch.
    genPrincipleMock.mockResolvedValueOnce([]);
    const matched = await runner.runStage(stage, {
      signal: fakeSignal,
      invariant: fakeInvariant,
      fixCandidate: fakeFix,
      recognized: matchedRecognize,
    });

    // C6 substrate path: no recognized → impl returns the canonical candidate.
    const unmatched = await runner.runStage(stage, {
      signal: fakeSignal,
      invariant: fakeInvariant,
      fixCandidate: fakeFix,
    });

    expect(matched.cid).not.toBe(unmatched.cid);
    expect(matched.output).toHaveLength(0);
    expect(unmatched.output).toHaveLength(1);
    expect(genPrincipleMock).toHaveBeenCalledTimes(2);
  });

  it("empty array (no principle generated) round-trips through the witness", async () => {
    const db = makeDb();
    genPrincipleMock.mockResolvedValue([]);
    const stage = makeGeneratePrincipleCandidateStage({
      db,
      llm: new StubLLMProvider(new Map()),
    });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      signal: fakeSignal,
      invariant: fakeInvariant,
      fixCandidate: fakeFix,
    });
    const b = await runner.runStage(stage, {
      signal: fakeSignal,
      invariant: fakeInvariant,
      fixCandidate: fakeFix,
    });

    expect(a.output).toHaveLength(0);
    expect(b.cacheHit).toBe(true);
    expect(b.output).toHaveLength(0);
    expect(genPrincipleMock).toHaveBeenCalledTimes(1);
  });

  it("dispatches via the registry as capability 'generatePrincipleCandidate'", async () => {
    const db = makeDb();
    const stage = makeGeneratePrincipleCandidateStage({
      db,
      llm: new StubLLMProvider(new Map()),
    });
    const registry = new InMemoryRegistry();
    registry.register(GENERATE_PRINCIPLE_CANDIDATE_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<
      GeneratePrincipleCandidateStageInput,
      PrincipleCandidate[]
    >(GENERATE_PRINCIPLE_CANDIDATE_CAPABILITY, {
      signal: fakeSignal,
      invariant: fakeInvariant,
      fixCandidate: fakeFix,
    });

    expect(result.output).toHaveLength(1);
    expect(result.output[0].kind).toBe("principle");
  });
});
