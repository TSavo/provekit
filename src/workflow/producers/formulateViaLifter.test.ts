/**
 * formulate-via-lifter Stage tests.
 *
 * Wires the StubLLMProvider to canned `.invariant.ts` source, runs the
 * Stage through the WorkflowRunner, and asserts the surface text +
 * lifted formula + propertyHash all flow into the cache + memento.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { StubLLMProvider } from "../../fix/types.js";
import type { IntentSignal } from "../../fix/types.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeFormulateViaLifterStage,
  FORMULATE_VIA_LIFTER_CAPABILITY,
  FormulateViaLifterError,
  type FormulateViaLifterStageInput,
  type FormulateViaLifterStageOutput,
} from "./formulateViaLifter.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "formulate-via-lifter-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-formulate-via-lifter-test-v1" };

const fakeIntent: IntentSignal = {
  source: "test",
  intentText: "denominator must not be zero",
  rawText: "denominator must not be zero",
  summary: "divide may crash on zero denominator",
  failureDescription: "ZeroDivisionError",
  codeReferences: [{ file: "src/math.ts", line: 1 }],
  classHint: "division-by-zero",
  bugClassHint: "division-by-zero",
};

const SURFACE_OK = `
import { property, forAll } from "provekit/ir";
import type { Int } from "provekit/sorts";
property("nonZeroDenominator", forAll<Int>((d) => d !== 0));
`;

const SURFACE_OK_FENCED = "```ts\n" + SURFACE_OK.trim() + "\n```";

const SURFACE_UNLIFTABLE = `
import { property } from "provekit/ir";
property("bad", somethingUnknown(42) === 0);
`;

const SURFACE_NO_PROPERTY = `
import { property } from "provekit/ir";
// no property() call here
`;

describe("formulate-via-lifter Stage", () => {
  let llm: StubLLMProvider;

  beforeEach(() => {
    llm = new StubLLMProvider(
      new Map([["You are writing invariants for a TypeScript codebase", SURFACE_OK]]),
    );
  });

  it("runs through WorkflowRunner.runStage and returns surfaceText + formula + propertyHash", async () => {
    const db = makeDb();
    const stage = makeFormulateViaLifterStage({ llm });
    const runner = new WorkflowRunner(db, wf);

    const input: FormulateViaLifterStageInput = { intent: fakeIntent };
    const result = await runner.runStage(stage, input);

    expect(result.cacheHit).toBe(false);
    const out = result.output;
    expect(out.name).toBe("nonZeroDenominator");
    expect(out.surfaceText).toContain("nonZeroDenominator");
    expect(out.formula.kind).toBe("forall");
    expect(out.propertyHash).toMatch(/^[0-9a-f]{16}$/);
    expect(out.inputCidsToCompose).toEqual([]);
  });

  it("strips Markdown fences from the LLM response before lifting", async () => {
    const fencedLLM = new StubLLMProvider(
      new Map([["You are writing invariants", SURFACE_OK_FENCED]]),
    );
    const stage = makeFormulateViaLifterStage({ llm: fencedLLM });
    const runner = new WorkflowRunner(makeDb(), wf);

    const result = await runner.runStage(stage, { intent: fakeIntent });
    expect(result.output.name).toBe("nonZeroDenominator");
    expect(result.output.surfaceText.startsWith("```")).toBe(false);
  });

  it("caches identical input — second run is a hit, LLM not invoked again", async () => {
    let calls = 0;
    const countingLLM: StubLLMProvider = new StubLLMProvider(new Map());
    countingLLM.complete = async () => {
      calls++;
      return SURFACE_OK;
    };
    const db = makeDb();
    const stage = makeFormulateViaLifterStage({ llm: countingLLM });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { intent: fakeIntent });
    const b = await runner.runStage(stage, { intent: fakeIntent });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(b.output.propertyHash).toBe(a.output.propertyHash);
    expect(calls).toBe(1);
  });

  it("dispatches via the registry as capability 'formulate-via-lifter'", async () => {
    const db = makeDb();
    const stage = makeFormulateViaLifterStage({ llm });
    const registry = new InMemoryRegistry();
    registry.register(FORMULATE_VIA_LIFTER_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<
      FormulateViaLifterStageInput,
      FormulateViaLifterStageOutput
    >(FORMULATE_VIA_LIFTER_CAPABILITY, { intent: fakeIntent });

    expect(result.output.name).toBe("nonZeroDenominator");
  });

  it("raises FormulateViaLifterError when surface contains an unliftable construct", async () => {
    const errLLM = new StubLLMProvider(
      new Map([["You are writing invariants", SURFACE_UNLIFTABLE]]),
    );
    const stage = makeFormulateViaLifterStage({ llm: errLLM });
    const runner = new WorkflowRunner(makeDb(), wf);

    await expect(runner.runStage(stage, { intent: fakeIntent })).rejects.toThrow(
      FormulateViaLifterError,
    );
  });

  it("raises FormulateViaLifterError when surface emits no property()", async () => {
    const emptyLLM = new StubLLMProvider(
      new Map([["You are writing invariants", SURFACE_NO_PROPERTY]]),
    );
    const stage = makeFormulateViaLifterStage({ llm: emptyLLM });
    const runner = new WorkflowRunner(makeDb(), wf);

    await expect(runner.runStage(stage, { intent: fakeIntent })).rejects.toThrow(
      /no property/,
    );
  });

  it("threads kitCatalogCids into the output's inputCidsToCompose, sorted", async () => {
    const stage = makeFormulateViaLifterStage({ llm });
    const runner = new WorkflowRunner(makeDb(), wf);

    const result = await runner.runStage(stage, {
      intent: fakeIntent,
      kitCatalogCids: ["bafy-z", "bafy-a", "bafy-m"],
    });
    expect(result.output.inputCidsToCompose).toEqual(["bafy-a", "bafy-m", "bafy-z"]);
  });
});
