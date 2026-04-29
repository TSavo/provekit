/**
 * Workflow runner smoke tests. Validates work-skipping cascade:
 * single-stage cache miss → hit, multi-stage pipeline where one
 * cached stage means all downstream stages also hit, and that
 * changing upstream input invalidates the downstream cache.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { WorkflowRunner } from "./runner.js";
import type { Stage, Workflow } from "./types.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "workflow-runner-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const dbPath = join(tmp, ".provekit", "test.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf: Workflow = { name: "test-workflow", cid: "wf-v1" };

interface CountingStage<I, O> extends Stage<I, O> {
  invocations: number;
}

function makeStage<I, O>(
  name: string,
  fn: (input: I) => O,
  options: { producedBy?: string } = {},
): CountingStage<I, O> {
  const stage: CountingStage<I, O> = {
    name,
    producedBy: options.producedBy ?? `${name}@1.0`,
    invocations: 0,
    serializeInput: (input) => input,
    serializeOutput: (output) => JSON.stringify(output),
    deserializeOutput: (witness) => JSON.parse(witness) as O,
    async run(input) {
      stage.invocations++;
      return fn(input);
    },
  };
  return stage;
}

describe("WorkflowRunner: single-stage cache", () => {
  let db: ReturnType<typeof makeDb>;
  beforeEach(() => {
    db = makeDb();
  });

  it("first run is a cache miss; second run with same input hits", async () => {
    const runner = new WorkflowRunner(db, wf);
    const stage = makeStage("double", (n: number) => n * 2);

    const first = await runner.runStage(stage, 7);
    expect(first.cacheHit).toBe(false);
    expect(first.output).toBe(14);
    expect(stage.invocations).toBe(1);

    const second = await runner.runStage(stage, 7);
    expect(second.cacheHit).toBe(true);
    expect(second.output).toBe(14);
    expect(second.cid).toBe(first.cid);
    expect(stage.invocations).toBe(1); // run() not called again
  });

  it("different inputs produce different cache slots", async () => {
    const runner = new WorkflowRunner(db, wf);
    const stage = makeStage("double", (n: number) => n * 2);

    const a = await runner.runStage(stage, 5);
    const b = await runner.runStage(stage, 6);

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(false);
    expect(a.cid).not.toBe(b.cid);
    expect(stage.invocations).toBe(2);
  });

  it("different stages with same input get separate cache slots", async () => {
    const runner = new WorkflowRunner(db, wf);
    const doubler = makeStage("double", (n: number) => n * 2);
    const tripler = makeStage("triple", (n: number) => n * 3);

    const d = await runner.runStage(doubler, 5);
    const t = await runner.runStage(tripler, 5);

    expect(d.output).toBe(10);
    expect(t.output).toBe(15);
    expect(d.cid).not.toBe(t.cid);
  });

  it("different workflows with same stage+input get separate cache slots", async () => {
    const runnerA = new WorkflowRunner(db, { name: "A", cid: "wf-a" });
    const runnerB = new WorkflowRunner(db, { name: "B", cid: "wf-b" });
    const stage = makeStage("double", (n: number) => n * 2);

    const a = await runnerA.runStage(stage, 5);
    const b = await runnerB.runStage(stage, 5);

    expect(a.output).toBe(10);
    expect(b.output).toBe(10);
    expect(a.cid).not.toBe(b.cid); // different binding (workflow CID)
    expect(stage.invocations).toBe(2);
  });
});

describe("WorkflowRunner: cascading work-skipping", () => {
  let db: ReturnType<typeof makeDb>;
  beforeEach(() => {
    db = makeDb();
  });

  it("a cached upstream stage means downstream stages also hit", async () => {
    const runner = new WorkflowRunner(db, wf);
    const s1 = makeStage("inc", (n: number) => n + 1);
    const s2 = makeStage("double", (n: number) => n * 2);
    const s3 = makeStage("negate", (n: number) => -n);

    // First run: 3 → inc(4) → double(8) → negate(-8)
    const r1a = await runner.runStage(s1, 3);
    const r2a = await runner.runStage(s2, r1a.output, [r1a.cid]);
    const r3a = await runner.runStage(s3, r2a.output, [r2a.cid]);

    expect(r1a.output).toBe(4);
    expect(r2a.output).toBe(8);
    expect(r3a.output).toBe(-8);
    expect(s1.invocations).toBe(1);
    expect(s2.invocations).toBe(1);
    expect(s3.invocations).toBe(1);

    // Second run, same input: every stage hits cache
    const r1b = await runner.runStage(s1, 3);
    const r2b = await runner.runStage(s2, r1b.output, [r1b.cid]);
    const r3b = await runner.runStage(s3, r2b.output, [r2b.cid]);

    expect(r1b.cacheHit).toBe(true);
    expect(r2b.cacheHit).toBe(true);
    expect(r3b.cacheHit).toBe(true);
    expect(r3b.output).toBe(-8);
    // No additional run() invocations — pure DB reads
    expect(s1.invocations).toBe(1);
    expect(s2.invocations).toBe(1);
    expect(s3.invocations).toBe(1);
  });

  it("changed input invalidates only that stage and its downstream", async () => {
    const runner = new WorkflowRunner(db, wf);
    const s1 = makeStage("inc", (n: number) => n + 1);
    const s2 = makeStage("double", (n: number) => n * 2);
    const s3 = makeStage("negate", (n: number) => -n);

    await runner.runStage(s1, 3).then(async (r1) => {
      const r2 = await runner.runStage(s2, r1.output, [r1.cid]);
      await runner.runStage(s3, r2.output, [r2.cid]);
    });

    // Run a second pipeline with different upstream input.
    // s1 with input=3 stays cached; s1 with input=5 is fresh, which
    // changes s2's input, which changes s3's input.
    const r1 = await runner.runStage(s1, 5);
    const r2 = await runner.runStage(s2, r1.output, [r1.cid]);
    const r3 = await runner.runStage(s3, r2.output, [r2.cid]);

    expect(r1.cacheHit).toBe(false);
    expect(r2.cacheHit).toBe(false);
    expect(r3.cacheHit).toBe(false);
    expect(r3.output).toBe(-12);
    expect(s1.invocations).toBe(2); // once for input=3, once for input=5
    expect(s2.invocations).toBe(2);
    expect(s3.invocations).toBe(2);
  });
});

describe("WorkflowRunner: DAG provenance", () => {
  let db: ReturnType<typeof makeDb>;
  beforeEach(() => {
    db = makeDb();
  });

  it("inputCids form a walkable DAG from terminal stage to leaf", async () => {
    const { walk } = await import("../fix/runtime/mementoStore.js");
    const runner = new WorkflowRunner(db, wf);
    const s1 = makeStage("inc", (n: number) => n + 1);
    const s2 = makeStage("double", (n: number) => n * 2);

    const r1 = await runner.runStage(s1, 3);
    const r2 = await runner.runStage(s2, r1.output, [r1.cid]);

    const provenance = walk(db, r2.cid);
    expect(provenance).toHaveLength(2);
    expect(provenance.map((m) => m.producedBy)).toEqual(["double@1.0", "inc@1.0"]);
  });
});
