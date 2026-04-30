import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { WorkflowRunner } from "../runner.js";
import {
  InMemoryRegistry,
  InMemoryActionRegistry,
} from "../registry.js";
import { parseManifest } from "../manifest.js";
import { findByCid } from "../../fix/runtime/mementoStore.js";
import { makeInvokeWorkflowAction } from "./invokeWorkflow.js";
import type { Stage } from "../types.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "invoke-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const SAMPLE_YAML = `
name: greet
cid: bafy-greet-v1
description: trivial test workflow
nodes:
  - id: greet
    capability: greet
    input:
      who: $input.who
output: $node.greet.output
`;

function makeGreetStage(): Stage<{ who: string }, { msg: string }> {
  return {
    name: "greet",
    producedBy: "greet@v1",
    serializeInput: (i) => ({ who: i.who }),
    serializeOutput: (o) => JSON.stringify(o),
    deserializeOutput: (w) => JSON.parse(w),
    async run(input) {
      return { msg: `hello ${input.who}` };
    },
  };
}

describe("invokeWorkflow Action", () => {
  it("invokes the located workflow via runManifest and returns its output", async () => {
    const db = makeDb();
    const action = makeInvokeWorkflowAction({ db });

    const manifest = parseManifest(SAMPLE_YAML);
    const factory = () => {
      const registry = new InMemoryRegistry();
      registry.register("greet", makeGreetStage());
      return { registry };
    };

    const result = await action.run({
      workflow: manifest,
      factories: { greet: factory },
      deps: {},
      workflowInput: { who: "world" },
    });

    expect(result.workflowName).toBe("greet");
    expect(result.workflowCid).toBe("bafy-greet-v1");
    expect(result.cacheHit).toBe(false);
    expect(result.output).toEqual({ msg: "hello world" });
    // Inner workflow's terminal Stage memento was persisted.
    const inner = findByCid(db, result.terminalCid);
    expect(inner).toBeTruthy();
  });

  it("hits the workflow-level cache on a second identical invocation", async () => {
    const db = makeDb();
    const action = makeInvokeWorkflowAction({ db });
    const manifest = parseManifest(SAMPLE_YAML);
    const factory = () => {
      const registry = new InMemoryRegistry();
      registry.register("greet", makeGreetStage());
      return { registry };
    };

    const first = await action.run({
      workflow: manifest,
      factories: { greet: factory },
      deps: {},
      workflowInput: { who: "world" },
    });
    expect(first.cacheHit).toBe(false);

    const second = await action.run({
      workflow: manifest,
      factories: { greet: factory },
      deps: {},
      workflowInput: { who: "world" },
    });
    expect(second.cacheHit).toBe(true);
    expect(second.output).toEqual(first.output);
  });

  it("throws when no factory is registered for the workflow", async () => {
    const db = makeDb();
    const action = makeInvokeWorkflowAction({ db });
    const manifest = parseManifest(SAMPLE_YAML);

    await expect(
      action.run({
        workflow: manifest,
        factories: {},
        deps: {},
        workflowInput: { who: "world" },
      }),
    ).rejects.toThrow(/no registry factory registered for workflow "greet"/);
  });

  it("threads deps into the factory", async () => {
    const db = makeDb();
    const action = makeInvokeWorkflowAction({ db });
    const manifest = parseManifest(SAMPLE_YAML);
    const seen: unknown[] = [];
    const factory = (deps: unknown) => {
      seen.push(deps);
      const registry = new InMemoryRegistry();
      registry.register("greet", makeGreetStage());
      return { registry };
    };
    await action.run({
      workflow: manifest,
      factories: { greet: factory },
      deps: { db, projectRoot: "/x" },
      workflowInput: { who: "world" },
    });
    expect(seen).toHaveLength(1);
    expect((seen[0] as { projectRoot: string }).projectRoot).toBe("/x");
  });

  it("describeResource produces a JSON snapshot", async () => {
    const db = makeDb();
    const action = makeInvokeWorkflowAction({ db });
    const desc = action.describeResource({
      workflowName: "greet",
      workflowCid: "bafy-greet-v1",
      terminalCid: "bafy-x",
      cacheHit: true,
      output: { msg: "hi" },
    });
    const parsed = JSON.parse(desc);
    expect(parsed.workflowName).toBe("greet");
    expect(parsed.cacheHit).toBe(true);
    expect(parsed.output).toBeUndefined();
  });

  it("audit memento captures workflow name+cid via the runner", async () => {
    const db = makeDb();
    const action = makeInvokeWorkflowAction({ db });
    const manifest = parseManifest(SAMPLE_YAML);
    const factory = () => {
      const registry = new InMemoryRegistry();
      registry.register("greet", makeGreetStage());
      return { registry };
    };

    const dispatcherWf = { name: "_dispatch", cid: "bafy-dispatch-test" };
    const runner = new WorkflowRunner(db, dispatcherWf);
    const audit = await runner.runAction(action, {
      workflow: manifest,
      factories: { greet: factory },
      deps: {},
      workflowInput: { who: "world" },
    });
    expect(audit.resource.workflowName).toBe("greet");
    expect(audit.auditCid).toBeTruthy();
  });
});
