/**
 * Investigate stage tests. Mocks investigate() to verify Stage wrapper
 * mechanics without spinning up project-tour / LLM. Round-trips an
 * InvestigateResult through the witness column.
 *
 * Note: investigate() writes a JSON file to disk as a side effect. The
 * Stage wrapper preserves reportPath in the cached output but does NOT
 * re-create the file on cache hit. These tests don't probe disk state
 * — they verify the Stage cache + dispatch contract.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";

const { investigateMock } = vi.hoisted(() => ({ investigateMock: vi.fn() }));
vi.mock("../../fix/stages/investigate.js", () => ({
  investigate: investigateMock,
}));

import { StubLLMProvider } from "../../fix/types.js";
import type { IntentSignal } from "../../fix/types.js";
import type { InvestigateResult } from "../../fix/stages/investigate.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeInvestigateStage,
  INVESTIGATE_CAPABILITY,
} from "./investigate.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "investigate-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-investigate-test-v1" };

const fakeSignal: IntentSignal = {
  source: "test",
  rawText: "denominator may be zero",
  summary: "divide may crash on zero denominator",
  failureDescription: "ZeroDivisionError",
  codeReferences: [{ file: "src/math.ts", line: 1 }],
  bugClassHint: "division-by-zero",
};

const fakeResult: InvestigateResult = {
  report: {
    symptomSummary: "Division crashes when b is 0.",
    rootCauseHypothesis: "Missing zero-denominator guard in calculate().",
    fixHypothesis: "Throw or short-circuit on b === 0 before dividing.",
    primaryLocation: {
      file: "src/math.ts",
      line: 4,
      function: "calculate",
      rationale: "The division operator is here.",
      confidence: "high",
    },
    candidateLocations: [],
  },
  reportPath: "/tmp/fake/.provekit/contexts/investigate-test.json",
  codeReferences: [{ file: "src/math.ts", line: 4, function: "calculate" }],
};

describe("investigate Stage", () => {
  beforeEach(() => {
    investigateMock.mockReset();
    investigateMock.mockResolvedValue(fakeResult);
  });

  it("runs through WorkflowRunner.runStage and returns the InvestigateResult", async () => {
    const db = makeDb();
    const stage = makeInvestigateStage({ llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      signal: fakeSignal,
      projectRoot: "/tmp/fake",
    });

    expect(result.cacheHit).toBe(false);
    expect(result.output.report.primaryLocation.file).toBe("src/math.ts");
    expect(result.output.codeReferences).toHaveLength(1);
    expect(investigateMock).toHaveBeenCalledTimes(1);
    const callArg = investigateMock.mock.calls[0][0];
    expect(callArg.signal).toBe(fakeSignal);
    expect(callArg.projectRoot).toBe("/tmp/fake");
  });

  it("caches identical input — second run is a hit, investigate() not invoked", async () => {
    const db = makeDb();
    const stage = makeInvestigateStage({ llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      signal: fakeSignal,
      projectRoot: "/tmp/fake",
    });
    const b = await runner.runStage(stage, {
      signal: fakeSignal,
      projectRoot: "/tmp/fake",
    });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(b.output.report.symptomSummary).toBe(a.output.report.symptomSummary);
    expect(investigateMock).toHaveBeenCalledTimes(1);
  });

  it("different projectRoot produces a different cache slot", async () => {
    const db = makeDb();
    const stage = makeInvestigateStage({ llm: new StubLLMProvider(new Map()) });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { signal: fakeSignal, projectRoot: "/tmp/a" });
    const b = await runner.runStage(stage, { signal: fakeSignal, projectRoot: "/tmp/b" });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(false);
    expect(a.cid).not.toBe(b.cid);
    expect(investigateMock).toHaveBeenCalledTimes(2);
  });

  it("dispatches via the registry as capability 'investigate'", async () => {
    const db = makeDb();
    const stage = makeInvestigateStage({ llm: new StubLLMProvider(new Map()) });
    const registry = new InMemoryRegistry();
    registry.register(INVESTIGATE_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<
      { signal: IntentSignal; projectRoot: string },
      InvestigateResult
    >(INVESTIGATE_CAPABILITY, { signal: fakeSignal, projectRoot: "/tmp/fake" });

    expect(result.output.report.primaryLocation.function).toBe("calculate");
  });
});
