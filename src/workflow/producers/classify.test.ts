/**
 * Classify stage tests. Mocks the underlying classify() function to
 * verify Stage wrapper mechanics without spinning up the remediation-
 * layer registry + LLM stack.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";

const { classifyMock } = vi.hoisted(() => ({ classifyMock: vi.fn() }));
vi.mock("../../fix/classify.js", () => ({ classify: classifyMock }));

import { StubLLMProvider } from "../../fix/types.js";
import type { BugLocus, IntentSignal, RemediationPlan } from "../../fix/types.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeClassifyStage,
  CLASSIFY_CAPABILITY,
} from "./classify.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "classify-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-classify-test-v1" };

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

const fakePlan: RemediationPlan = {
  signal: fakeSignal,
  locus: fakeLocus,
  primaryLayer: "input-validation",
  secondaryLayers: [],
  artifacts: [],
  rationale: "denominator should be guarded at call site",
};

describe("classify Stage", () => {
  beforeEach(() => {
    classifyMock.mockReset();
    classifyMock.mockResolvedValue(fakePlan);
  });

  it("runs through WorkflowRunner.runStage and returns the RemediationPlan", async () => {
    const db = makeDb();
    const stage = makeClassifyStage({ llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      signal: fakeSignal,
      locus: fakeLocus,
    });

    expect(result.cacheHit).toBe(false);
    expect(result.output.primaryLayer).toBe("input-validation");
    expect(classifyMock).toHaveBeenCalledTimes(1);
    const [signalArg, locusArg] = classifyMock.mock.calls[0];
    expect(signalArg).toBe(fakeSignal);
    expect(locusArg).toBe(fakeLocus);
  });

  it("caches identical input — second run is a hit", async () => {
    const db = makeDb();
    const stage = makeClassifyStage({ llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { signal: fakeSignal, locus: fakeLocus });
    const b = await runner.runStage(stage, { signal: fakeSignal, locus: fakeLocus });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(b.output.primaryLayer).toBe(a.output.primaryLayer);
    expect(classifyMock).toHaveBeenCalledTimes(1);
  });

  it("null locus and undefined locus collapse to the same hash", async () => {
    const db = makeDb();
    const stage = makeClassifyStage({ llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { signal: fakeSignal, locus: null });
    const b = await runner.runStage(stage, {
      signal: fakeSignal,
      locus: undefined as unknown as null,
    });

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
  });

  it("dispatches via the registry as capability 'classify'", async () => {
    const db = makeDb();
    const stage = makeClassifyStage({ llm: new StubLLMProvider(new Map()) });
    const registry = new InMemoryRegistry();
    registry.register(CLASSIFY_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<
      { signal: IntentSignal; locus: BugLocus | null },
      RemediationPlan
    >(CLASSIFY_CAPABILITY, { signal: fakeSignal, locus: fakeLocus });

    expect(result.output.primaryLayer).toBe("input-validation");
  });
});
