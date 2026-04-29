import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { WorkflowRunner } from "./runner.js";
import { InMemoryRegistry } from "./registry.js";
import {
  parseManifest,
  validateManifest,
  topoSort,
  runManifest,
  manifestToWorkflow,
} from "./manifest.js";
import type { Stage } from "./types.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "manifest-runner-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

function makeStage<I, O>(name: string, fn: (input: I) => O): Stage<I, O> & { invocations: number } {
  const stage = {
    name,
    producedBy: `${name}@1.0`,
    invocations: 0,
    serializeInput: (input: I) => input,
    serializeOutput: (output: O) => JSON.stringify(output),
    deserializeOutput: (witness: string) => JSON.parse(witness) as O,
    async run(input: I) {
      stage.invocations++;
      return fn(input);
    },
  };
  return stage;
}

describe("parseManifest + validateManifest", () => {
  it("parses a minimal valid manifest", () => {
    const yaml = `
name: trivial
cid: wf-trivial-v1
nodes:
  - id: only
    capability: passthrough
    input: $input
output: $node.only.output
`;
    const m = parseManifest(yaml);
    expect(m.name).toBe("trivial");
    expect(m.nodes).toHaveLength(1);
    expect(m.output).toBe("$node.only.output");
  });

  it("preserves description and complex input shapes", () => {
    const yaml = `
name: complex
cid: wf-complex-v1
description: tests composed inputs
nodes:
  - id: a
    capability: cap-a
    input: $input
  - id: b
    capability: cap-b
    input:
      from_a: $node.a.output
      from_input: $input.x
      literal: 42
output: $node.b.output
`;
    const m = parseManifest(yaml);
    expect(m.description).toBe("tests composed inputs");
    expect(m.nodes[1].input).toEqual({
      from_a: "$node.a.output",
      from_input: "$input.x",
      literal: 42,
    });
  });

  it("rejects manifest with duplicate node ids", () => {
    expect(() =>
      validateManifest({
        name: "x",
        cid: "y",
        output: "$node.a.output",
        nodes: [
          { id: "a", capability: "c", input: "$input" },
          { id: "a", capability: "c", input: "$input" },
        ],
      }),
    ).toThrow(/duplicate node id/);
  });

  it("rejects references to undeclared nodes", () => {
    expect(() =>
      validateManifest({
        name: "x",
        cid: "y",
        output: "$node.missing.output",
        nodes: [{ id: "a", capability: "c", input: "$input" }],
      }),
    ).toThrow(/undeclared node "missing"/);
  });

  it("rejects cyclic graphs", () => {
    expect(() =>
      validateManifest({
        name: "x",
        cid: "y",
        output: "$node.a.output",
        nodes: [
          { id: "a", capability: "c", input: "$node.b.output" },
          { id: "b", capability: "c", input: "$node.a.output" },
        ],
      }),
    ).toThrow(/cycle detected/);
  });

  it("rejects output that doesn't reference a node", () => {
    expect(() =>
      validateManifest({
        name: "x",
        cid: "y",
        output: "literal-not-ref",
        nodes: [{ id: "a", capability: "c", input: "$input" }],
      }),
    ).toThrow(/\$-prefixed reference/);
  });
});

describe("topoSort", () => {
  it("orders by dependency", () => {
    const nodes = [
      { id: "c", capability: "x", input: "$node.b.output" },
      { id: "a", capability: "x", input: "$input" },
      { id: "b", capability: "x", input: "$node.a.output" },
    ];
    const ordered = topoSort(nodes).map((n) => n.id);
    expect(ordered).toEqual(["a", "b", "c"]);
  });

  it("handles parallel branches", () => {
    const nodes = [
      { id: "root", capability: "x", input: "$input" },
      { id: "left", capability: "x", input: "$node.root.output" },
      { id: "right", capability: "x", input: "$node.root.output" },
      {
        id: "merge",
        capability: "x",
        input: { l: "$node.left.output", r: "$node.right.output" },
      },
    ];
    const ordered = topoSort(nodes).map((n) => n.id);
    expect(ordered[0]).toBe("root");
    expect(ordered[3]).toBe("merge");
    expect(ordered.slice(1, 3).sort()).toEqual(["left", "right"]);
  });
});

describe("runManifest", () => {
  let db: ReturnType<typeof makeDb>;
  beforeEach(() => {
    db = makeDb();
  });

  it("executes a 3-stage pipeline driven by YAML", async () => {
    const yaml = `
name: pipeline
cid: wf-pipe-v1
nodes:
  - id: inc
    capability: inc
    input: $input
  - id: double
    capability: double
    input: $node.inc.output
  - id: negate
    capability: negate
    input: $node.double.output
output: $node.negate.output
`;
    const manifest = parseManifest(yaml);
    const registry = new InMemoryRegistry();
    const inc = makeStage("inc", (n: number) => n + 1);
    const dbl = makeStage("double", (n: number) => n * 2);
    const neg = makeStage("negate", (n: number) => -n);
    registry.register("inc", inc);
    registry.register("double", dbl);
    registry.register("negate", neg);

    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);
    const result = await runManifest(runner, registry, manifest, 3);

    expect(result.output).toBe(-8); // 3 → 4 → 8 → -8
    expect(inc.invocations).toBe(1);
    expect(dbl.invocations).toBe(1);
    expect(neg.invocations).toBe(1);

    // Re-run with same input: workflow-level cache hit, zero stage invocations.
    const second = await runManifest(runner, registry, manifest, 3);
    expect(second.cacheHit).toBe(true);
    expect(second.output).toBe(-8);
    expect(inc.invocations).toBe(1);
    expect(dbl.invocations).toBe(1);
    expect(neg.invocations).toBe(1);
  });

  it("composes inputs from multiple upstream nodes", async () => {
    const yaml = `
name: merge
cid: wf-merge-v1
nodes:
  - id: a
    capability: identity
    input: $input.x
  - id: b
    capability: identity
    input: $input.y
  - id: sum
    capability: sum
    input:
      a: $node.a.output
      b: $node.b.output
output: $node.sum.output
`;
    const manifest = parseManifest(yaml);
    const registry = new InMemoryRegistry();
    registry.register("identity", makeStage("identity", (n: number) => n));
    registry.register(
      "sum",
      makeStage("sum", (input: { a: number; b: number }) => input.a + input.b),
    );

    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);
    const result = await runManifest(runner, registry, manifest, { x: 5, y: 7 });
    expect(result.output).toBe(12);
  });

  it("throws when a referenced capability isn't registered", async () => {
    const yaml = `
name: missing
cid: wf-missing-v1
nodes:
  - id: x
    capability: not-registered
    input: $input
output: $node.x.output
`;
    const manifest = parseManifest(yaml);
    const registry = new InMemoryRegistry();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);
    await expect(runManifest(runner, registry, manifest, null)).rejects.toThrow(
      /not-registered.*not registered/,
    );
  });

  it("threads CIDs into inputCids so the DAG walks correctly", async () => {
    const { walk } = await import("../fix/runtime/mementoStore.js");
    const yaml = `
name: walkable
cid: wf-walk-v1
nodes:
  - id: a
    capability: inc
    input: $input
  - id: b
    capability: double
    input: $node.a.output
output: $node.b.output
`;
    const manifest = parseManifest(yaml);
    const registry = new InMemoryRegistry();
    registry.register("inc", makeStage("inc", (n: number) => n + 1));
    registry.register("double", makeStage("double", (n: number) => n * 2));

    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);
    const result = await runManifest(runner, registry, manifest, 5);

    // walk from workflow root reaches workflow + b + a (3 nodes)
    const provenance = walk(db, result.cid);
    expect(provenance).toHaveLength(3);
    const producers = provenance.map((m) => m.producedBy);
    expect(producers[0]).toMatch(/^workflow:walkable@/);
    expect(producers).toContain("double@1.0");
    expect(producers).toContain("inc@1.0");
  });
});
