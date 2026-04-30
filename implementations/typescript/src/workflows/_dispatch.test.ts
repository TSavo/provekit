import { describe, it, expect } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
} from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest } from "../workflow/manifest.js";
import {
  loadDispatchManifest,
  registerDispatchRegistries,
  discoverWorkflows,
  WORKFLOWS_DIR,
  DISPATCH_MANIFEST_PATH,
} from "./_dispatch.js";
import { InMemoryRegistry } from "../workflow/registry.js";
import type { Stage } from "../workflow/types.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "dispatch-test-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

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

describe("loadDispatchManifest", () => {
  it("parses the on-disk dispatcher manifest", () => {
    const m = loadDispatchManifest();
    expect(m.name).toBe("_dispatch");
    expect(m.cid).toBe("bafy-dispatch-v1");
    const nodeCaps = m.nodes.map((n) => n.capability).sort();
    expect(nodeCaps).toEqual(["locate-workflow", "parse-argv"]);
    expect(m.actions).toHaveLength(1);
    expect(m.actions![0]!.action).toBe("invoke-workflow");
  });

  it("defines DISPATCH_MANIFEST_PATH at the conventional location", () => {
    expect(DISPATCH_MANIFEST_PATH).toBe(join(WORKFLOWS_DIR, "_dispatch.workflow.yaml"));
  });
});

describe("registerDispatchRegistries", () => {
  it("registers parse-argv + locate-workflow + invoke-workflow", () => {
    const db = makeDb();
    const { registry, actionRegistry } = registerDispatchRegistries({ db });
    expect(registry.capabilities()).toEqual(["locate-workflow", "parse-argv"]);
    expect(actionRegistry.capabilities()).toEqual(["invoke-workflow"]);
  });
});

describe("discoverWorkflows", () => {
  it("walks the workflows directory and indexes every YAML by name", () => {
    const tmp = mkdtempSync(join(tmpdir(), "discover-"));
    writeFileSync(
      join(tmp, "alpha.workflow.yaml"),
      [
        "name: alpha",
        "cid: bafy-alpha-v1",
        "cli:",
        "  description: alpha command",
        "  args:",
        "    - { name: x, positional: true, required: true, type: string }",
        "nodes:",
        "  - id: only",
        "    capability: noop",
        "    input:",
        "      x: $input.x",
        "output: $node.only.output",
        "",
      ].join("\n"),
    );
    writeFileSync(
      join(tmp, "beta.workflow.yaml"),
      [
        "name: beta",
        "cid: bafy-beta-v1",
        "nodes:",
        "  - id: only",
        "    capability: noop",
        "    input: {}",
        "output: $node.only.output",
        "",
      ].join("\n"),
    );
    writeFileSync(
      join(tmp, "_internal.workflow.yaml"),
      [
        "name: _internal",
        "cid: bafy-internal-v1",
        "cli:",
        "  description: internal — should not appear",
        "nodes:",
        "  - id: only",
        "    capability: noop",
        "    input: {}",
        "output: $node.only.output",
        "",
      ].join("\n"),
    );

    const { cliBlocks, manifestPaths } = discoverWorkflows(tmp);

    // _internal is excluded from BOTH (it's a dispatcher internal).
    expect(Object.keys(manifestPaths).sort()).toEqual(["alpha", "beta"]);

    // Only workflows declaring a cli: block appear in cliBlocks.
    expect(Object.keys(cliBlocks).sort()).toEqual(["alpha"]);
    expect(cliBlocks.alpha!.description).toBe("alpha command");
  });

  it("returns empty maps when the directory does not exist", () => {
    const { cliBlocks, manifestPaths } = discoverWorkflows("/nonexistent/path");
    expect(cliBlocks).toEqual({});
    expect(manifestPaths).toEqual({});
  });
});

describe("dispatcher end-to-end", () => {
  it("dispatches argv through parse-argv → locate-workflow → invoke-workflow", async () => {
    const db = makeDb();

    // Build a tmp workflows directory with one greet.workflow.yaml.
    const tmp = mkdtempSync(join(tmpdir(), "dispatch-e2e-"));
    const greetYaml = [
      "name: greet",
      "cid: bafy-greet-e2e-v1",
      "cli:",
      "  description: greet someone",
      "  args:",
      "    - { name: who, positional: true, required: true, type: string }",
      "nodes:",
      "  - id: greet",
      "    capability: greet",
      "    input:",
      "      who: $input.who",
      "output: $node.greet.output",
      "",
    ].join("\n");
    const greetPath = join(tmp, "greet.workflow.yaml");
    writeFileSync(greetPath, greetYaml);

    const { cliBlocks, manifestPaths } = discoverWorkflows(tmp);
    expect(Object.keys(cliBlocks)).toContain("greet");
    expect(manifestPaths.greet).toBe(greetPath);

    const factories = {
      greet: () => {
        const r = new InMemoryRegistry();
        r.register("greet", makeGreetStage());
        return { registry: r };
      },
    };

    const manifest = loadDispatchManifest();
    const { registry, actionRegistry } = registerDispatchRegistries({ db });
    const runner = new WorkflowRunner(
      db,
      { name: manifest.name, cid: manifest.cid },
      registry,
    );

    await runManifest(
      runner,
      registry,
      manifest,
      {
        argv: ["greet", "world"],
        cliBlocks,
        manifestPaths,
        factories,
        deps: {},
      },
      actionRegistry,
    );

    // Verify the inner Stage memento was written by the inner runManifest.
    const { findMementoByPropertyHash } = await import(
      "../fix/runtime/mementoStore.js"
    );
    const { hashCanonical } = await import("../fix/runtime/mementoStore.js");
    const inner = findMementoByPropertyHash(
      db,
      hashCanonical({ who: "world" }),
    );
    expect(inner.length).toBeGreaterThan(0);
  });

  it("rejects underscore-prefixed commands at parse-argv", async () => {
    const db = makeDb();
    const manifest = loadDispatchManifest();
    const { registry, actionRegistry } = registerDispatchRegistries({ db });
    const runner = new WorkflowRunner(
      db,
      { name: manifest.name, cid: manifest.cid },
      registry,
    );

    // parse-argv returns kind:"unknown" for "_dispatch"; locate-workflow
    // then trips on the empty command and throws. The dispatcher manifest
    // is a happy-path graph; cli.ts intercepts kind:"help"/"unknown"
    // before invoking the manifest.
    await expect(
      runManifest(
        runner,
        registry,
        manifest,
        {
          argv: ["_dispatch"],
          cliBlocks: {},
          manifestPaths: {},
          factories: {},
          deps: {},
        },
        actionRegistry,
      ),
    ).rejects.toThrow();
  });
});
