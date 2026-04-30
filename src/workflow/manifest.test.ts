import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { WorkflowRunner } from "./runner.js";
import { InMemoryRegistry, InMemoryActionRegistry } from "./registry.js";
import {
  parseManifest,
  validateManifest,
  topoSort,
  runManifest,
  manifestToWorkflow,
  loadKitsLock,
} from "./manifest.js";
import type { Action, Stage } from "./types.js";

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

function makeAction<I, R>(
  name: string,
  fn: (input: I) => R,
): Action<I, R> & { invocations: number } {
  const action = {
    name,
    producedBy: `${name}@1.0`,
    invocations: 0,
    serializeInput: (input: I) => input,
    describeResource: (r: R) => JSON.stringify(r),
    async run(input: I) {
      action.invocations++;
      return fn(input);
    },
  };
  return action;
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

  it("preserves a cli: block when present", () => {
    const yaml = `
name: hookable
cid: wf-hookable-v1
cli:
  description: install or remove the git hook
  args:
    - name: project
      positional: true
      type: path
      default: "."
    - name: uninstall
      flag: true
nodes:
  - id: only
    capability: passthrough
    input: $input
output: $node.only.output
`;
    const m = parseManifest(yaml);
    expect(m.cli).toBeDefined();
    expect(m.cli!.description).toBe("install or remove the git hook");
    expect(m.cli!.args).toHaveLength(2);
    expect(m.cli!.args![0]).toEqual({
      name: "project", positional: true, type: "path", default: ".",
    });
    expect(m.cli!.args![1]).toEqual({ name: "uninstall", flag: true });
  });

  it("omits cli when not present", () => {
    const m = validateManifest({
      name: "x", cid: "y",
      output: "$node.a.output",
      nodes: [{ id: "a", capability: "c", input: "$input" }],
    });
    expect(m.cli).toBeUndefined();
  });

  it("rejects malformed cli.description", () => {
    expect(() =>
      validateManifest({
        name: "x", cid: "y",
        output: "$node.a.output",
        nodes: [{ id: "a", capability: "c", input: "$input" }],
        cli: { args: [] },
      }),
    ).toThrow(/cli.description must be a string/);
  });

  it("rejects unknown cli arg type", () => {
    expect(() =>
      validateManifest({
        name: "x", cid: "y",
        output: "$node.a.output",
        nodes: [{ id: "a", capability: "c", input: "$input" }],
        cli: { description: "x", args: [{ name: "n", type: "bogus" }] },
      }),
    ).toThrow(/cli.args\[0\].type must be one of/);
  });
});

describe("topoSort", () => {
  it("orders by dependency", () => {
    const nodes = [
      { id: "c", capability: "x", input: "$node.b.output" },
      { id: "a", capability: "x", input: "$input" },
      { id: "b", capability: "x", input: "$node.a.output" },
    ];
    const ordered = topoSort(nodes).map((e) => e.spec.id);
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
    const ordered = topoSort(nodes).map((e) => e.spec.id);
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

describe("actions: block (Stages-vs-Actions grammar)", () => {
  it("parses a manifest with an actions: block", () => {
    const yaml = `
name: with-actions
cid: wf-actions-v1
nodes:
  - id: consume
    capability: consume
    input:
      overlay: $action.open-overlay.resource
actions:
  - id: open-overlay
    action: open-overlay
    input:
      baseRef: $input.baseRef
output: $node.consume.output
`;
    const m = parseManifest(yaml);
    expect(m.actions).toHaveLength(1);
    expect(m.actions[0]).toMatchObject({
      id: "open-overlay",
      action: "open-overlay",
    });
  });

  it("defaults manifest.actions to [] when absent", () => {
    const yaml = `
name: noActions
cid: wf-no-actions
nodes:
  - id: only
    capability: passthrough
    input: $input
output: $node.only.output
`;
    const m = parseManifest(yaml);
    expect(m.actions).toEqual([]);
  });

  it("rejects $action.<id>.output with a clear error", () => {
    expect(() =>
      validateManifest({
        name: "x",
        cid: "y",
        output: "$node.consume.output",
        nodes: [
          {
            id: "consume",
            capability: "consume",
            input: { bad: "$action.open-overlay.output" },
          },
        ],
        actions: [
          {
            id: "open-overlay",
            action: "open-overlay",
            input: "$input",
          },
        ],
      }),
    ).toThrow(/must end in \.resource/);
  });

  it("rejects references to undeclared actions", () => {
    expect(() =>
      validateManifest({
        name: "x",
        cid: "y",
        output: "$node.consume.output",
        nodes: [
          {
            id: "consume",
            capability: "consume",
            input: { overlay: "$action.missing.resource" },
          },
        ],
        actions: [],
      }),
    ).toThrow(/undeclared action "missing"/);
  });

  it("rejects ids colliding between nodes and actions", () => {
    expect(() =>
      validateManifest({
        name: "x",
        cid: "y",
        output: "$node.shared.output",
        nodes: [{ id: "shared", capability: "c", input: "$input" }],
        actions: [{ id: "shared", action: "a", input: "$input" }],
      }),
    ).toThrow(/duplicate id "shared"/);
  });

  it("rejects manifest.output pointing at an action resource", () => {
    expect(() =>
      validateManifest({
        name: "x",
        cid: "y",
        output: "$action.open-overlay.resource",
        nodes: [],
        actions: [
          { id: "open-overlay", action: "open-overlay", input: "$input" },
        ],
      }),
    ).toThrow(/action resources are not cacheable workflow outputs/);
  });

  it("rejects cycles between a stage and an action", () => {
    // node consume → action overlay → node consume (cycle)
    expect(() =>
      validateManifest({
        name: "x",
        cid: "y",
        output: "$node.consume.output",
        nodes: [
          {
            id: "consume",
            capability: "consume",
            input: { overlay: "$action.overlay.resource" },
          },
        ],
        actions: [
          {
            id: "overlay",
            action: "overlay",
            input: { signal: "$node.consume.output" },
          },
        ],
      }),
    ).toThrow(/cycle detected/);
  });

  it("topo-sorts mixed Stage/Action graphs by dependency", () => {
    const order = topoSort(
      [
        {
          id: "consume",
          capability: "consume",
          input: { overlay: "$action.open-overlay.resource" },
        },
      ],
      [
        {
          id: "open-overlay",
          action: "open-overlay",
          input: "$input",
        },
      ],
    );
    expect(order).toHaveLength(2);
    expect(order[0]).toMatchObject({ kind: "action" });
    expect((order[0].spec as { id: string }).id).toBe("open-overlay");
    expect(order[1]).toMatchObject({ kind: "node" });
    expect((order[1].spec as { id: string }).id).toBe("consume");
  });

  it("respects runAfter as an ordering constraint", () => {
    // cleanup runs after stage finishes, even though it has no data ref
    const order = topoSort(
      [{ id: "work", capability: "work", input: "$input" }],
      [
        {
          id: "cleanup",
          action: "cleanup",
          input: "$input",
          runAfter: "$node.work",
        },
      ],
    );
    expect(order).toHaveLength(2);
    expect((order[0].spec as { id: string }).id).toBe("work");
    expect((order[1].spec as { id: string }).id).toBe("cleanup");
  });

  it("rejects runAfter referencing an undeclared id", () => {
    expect(() =>
      validateManifest({
        name: "x",
        cid: "y",
        output: "$node.work.output",
        nodes: [{ id: "work", capability: "work", input: "$input" }],
        actions: [
          {
            id: "cleanup",
            action: "cleanup",
            input: "$input",
            runAfter: "$node.missing",
          },
        ],
      }),
    ).toThrow(/undeclared node "missing"/);
  });

  it("rejects malformed runAfter (3-part) references", () => {
    expect(() =>
      validateManifest({
        name: "x",
        cid: "y",
        output: "$node.work.output",
        nodes: [{ id: "work", capability: "work", input: "$input" }],
        actions: [
          {
            id: "cleanup",
            action: "cleanup",
            input: "$input",
            runAfter: "$node.work.output",
          },
        ],
      }),
    ).toThrow(/malformed runAfter reference/);
  });
});

describe("runManifest with actions", () => {
  let db: ReturnType<typeof makeDb>;
  beforeEach(() => {
    db = makeDb();
  });

  it("runs an action and threads its resource into a downstream stage", async () => {
    const yaml = `
name: with-action
cid: wf-action-run-v1
nodes:
  - id: consume
    capability: consume
    input:
      overlay: $action.open-overlay.resource
      n: $input
actions:
  - id: open-overlay
    action: open-overlay
    input: $input
output: $node.consume.output
`;
    const manifest = parseManifest(yaml);
    const stages = new InMemoryRegistry();
    const actions = new InMemoryActionRegistry();

    const overlayAction = makeAction("open-overlay", (n: number) => ({
      worktreePath: `/tmp/overlay-${n}`,
      baseRef: `ref-${n}`,
    }));
    actions.register("open-overlay", overlayAction);

    const consumeStage = makeStage(
      "consume",
      (input: { overlay: { worktreePath: string }; n: number }) =>
        `consumed ${input.overlay.worktreePath} with ${input.n}`,
    );
    stages.register("consume", consumeStage);

    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), stages);
    const result = await runManifest(runner, stages, manifest, 7, actions);
    expect(result.output).toBe("consumed /tmp/overlay-7 with 7");
    expect(overlayAction.invocations).toBe(1);
    expect(consumeStage.invocations).toBe(1);

    // Re-run: workflow-level cache hit, NEITHER stage nor action runs.
    // (Action audit cids do not affect Stage propertyHashes, so the
    // workflow-level memento sees byte-identical input and short-circuits.)
    const second = await runManifest(runner, stages, manifest, 7, actions);
    expect(second.cacheHit).toBe(true);
    expect(overlayAction.invocations).toBe(1);
    expect(consumeStage.invocations).toBe(1);
  });

  it("fails fast when an action capability is not registered", async () => {
    const yaml = `
name: missing-action
cid: wf-missing-action-v1
nodes:
  - id: noop
    capability: noop
    input: $input
actions:
  - id: dangling
    action: not-registered
    input: $input
output: $node.noop.output
`;
    const manifest = parseManifest(yaml);
    const stages = new InMemoryRegistry();
    stages.register("noop", makeStage("noop", (n: unknown) => n));
    const actions = new InMemoryActionRegistry();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), stages);
    await expect(
      runManifest(runner, stages, manifest, null, actions),
    ).rejects.toThrow(/not-registered.*not registered/);
  });

  it("requires an actionRegistry when the manifest declares actions", async () => {
    const yaml = `
name: needs-action-registry
cid: wf-needs-action-registry-v1
nodes:
  - id: noop
    capability: noop
    input: $input
actions:
  - id: a
    action: a
    input: $input
output: $node.noop.output
`;
    const manifest = parseManifest(yaml);
    const stages = new InMemoryRegistry();
    stages.register("noop", makeStage("noop", (n: unknown) => n));
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), stages);
    await expect(runManifest(runner, stages, manifest, null)).rejects.toThrow(
      /declares actions but runManifest was called without an actionRegistry/,
    );
  });
});

describe("loadKitsLock", () => {
  let tmp: string;
  beforeEach(() => {
    tmp = mkdtempSync(join(tmpdir(), "kitslock-"));
  });

  it("returns null when .provekit/kits.lock does not exist", () => {
    expect(loadKitsLock(tmp)).toBeNull();
  });

  it("parses a valid lockfile", () => {
    mkdirSync(join(tmp, ".provekit"), { recursive: true });
    writeFileSync(
      join(tmp, ".provekit", "kits.lock"),
      `typescript: { version: "0.5.2", cid: "abc123" }
rust: { version: "0.1.0", cid: "def456" }
`,
    );
    const lock = loadKitsLock(tmp);
    expect(lock).toEqual({
      typescript: { version: "0.5.2", cid: "abc123" },
      rust: { version: "0.1.0", cid: "def456" },
    });
  });

  it("throws on a malformed entry (missing version)", () => {
    mkdirSync(join(tmp, ".provekit"), { recursive: true });
    writeFileSync(
      join(tmp, ".provekit", "kits.lock"),
      `typescript: { cid: "abc123" }
`,
    );
    expect(() => loadKitsLock(tmp)).toThrow(/version must be a string/);
  });

  it("returns {} for an empty lockfile", () => {
    mkdirSync(join(tmp, ".provekit"), { recursive: true });
    writeFileSync(join(tmp, ".provekit", "kits.lock"), "");
    expect(loadKitsLock(tmp)).toEqual({});
  });
});
