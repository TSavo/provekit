/**
 * Formulate stage tests. Validates the Stage wrapper mechanics
 * (serializeInput canonicalization, output round-trip, cache key
 * shape, registry dispatch). The underlying formulateInvariant()
 * function has its own extensive test suite; this file verifies
 * the Stage contract, not the SMT/Z3 internals.
 *
 * Strategy: vi.mock formulateInvariant so we don't need to spin
 * up SAST + DSL eval + Z3 inside a Stage smoke test. The Stage
 * just has to wire input through and witness back.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";

const { formulateInvariantMock } = vi.hoisted(() => ({
  formulateInvariantMock: vi.fn(),
}));
vi.mock("../../fix/stages/formulateInvariant.js", () => ({
  formulateInvariant: formulateInvariantMock,
}));

import { StubLLMProvider } from "../../fix/types.js";
import type { BugLocus, IntentSignal, InvariantClaim } from "../../fix/types.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeFormulateStage,
  FORMULATE_CAPABILITY,
} from "./formulate.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "formulate-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-formulate-test-v1" };

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

const fakeClaim: InvariantClaim = {
  principleId: null,
  description: "denominator must not be zero",
  formalExpression: "(declare-const b Int) (assert (= b 0))",
  bindings: [
    { smt_constant: "b", source_expr: "b", file: "src/math.ts", line: 1 } as InvariantClaim["bindings"][number],
  ],
  complexity: 3,
  witness: "(model (define-fun b () Int 0))",
  source: "llm",
};

describe("formulate Stage", () => {
  beforeEach(() => {
    formulateInvariantMock.mockReset();
    formulateInvariantMock.mockResolvedValue(fakeClaim);
  });

  it("runs through WorkflowRunner.runStage and returns the InvariantClaim", async () => {
    const db = makeDb();
    const stage = makeFormulateStage({ db, llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      signal: fakeSignal,
      locus: fakeLocus,
    });

    expect(result.cacheHit).toBe(false);
    expect(result.output.formalExpression).toBe(fakeClaim.formalExpression);
    expect(result.output.bindings).toHaveLength(1);
    expect(formulateInvariantMock).toHaveBeenCalledTimes(1);
    const callArg = formulateInvariantMock.mock.calls[0][0];
    expect(callArg.signal).toBe(fakeSignal);
    expect(callArg.locus).toBe(fakeLocus);
    expect(callArg.db).toBe(db);
  });

  it("caches identical input — second run is a hit, formulateInvariant not invoked again", async () => {
    const db = makeDb();
    const stage = makeFormulateStage({ db, llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { signal: fakeSignal, locus: fakeLocus });
    const b = await runner.runStage(stage, { signal: fakeSignal, locus: fakeLocus });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(formulateInvariantMock).toHaveBeenCalledTimes(1);
    // Round-trip: cache-reconstructed output equals freshly computed.
    expect(b.output.formalExpression).toBe(a.output.formalExpression);
    expect(b.output.bindings).toEqual(a.output.bindings);
  });

  it("treats undefined recognized/investigateReport as canonical (same hash)", async () => {
    const db = makeDb();
    const stage = makeFormulateStage({ db, llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { signal: fakeSignal, locus: fakeLocus });
    // Pass undefined explicitly — should still hit the same cache slot.
    const b = await runner.runStage(stage, {
      signal: fakeSignal,
      locus: fakeLocus,
      recognized: undefined,
      investigateReport: undefined,
    });
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
  });

  it("dispatches via the registry as capability 'formulate'", async () => {
    const db = makeDb();
    const stage = makeFormulateStage({ db, llm: new StubLLMProvider(new Map()) });
    const registry = new InMemoryRegistry();
    registry.register(FORMULATE_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<
      { signal: IntentSignal; locus: BugLocus },
      InvariantClaim
    >(FORMULATE_CAPABILITY, { signal: fakeSignal, locus: fakeLocus });

    expect(result.output.formalExpression).toBe(fakeClaim.formalExpression);
  });
});
