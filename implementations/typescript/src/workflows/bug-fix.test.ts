/**
 * Bug-fix workflow smoke test.
 *
 * Asserts:
 *   - The YAML manifest parses + structurally validates.
 *   - `registerBugFixRegistries` produces a registry pair with the 10
 *     stage capabilities and the 1 action capability the manifest names.
 *   - The manifest's stage + action capability sets match the documented
 *     constants.
 *   - `runManifest` dispatches the first stage (intake) end-to-end with
 *     a stubbed LLM via the registered subset.
 *
 * Most stages need real fixtures (a git repo, a real SAST DB, a real
 * agent-mode LLM) to run; this test does NOT exercise them. It proves
 * the manifest + registries are wired correctly and that workflow
 * dispatch reaches at least the first node. The full end-to-end smoke
 * lives at src/integration/bug-fix-workflow.smoke.test.ts.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb, type Db } from "../db/index.js";
import { _clearIntakeRegistry } from "../fix/intake.js";
import { registerAll } from "../fix/intakeAdapters/index.js";
import { StubLLMProvider } from "../fix/types.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import {
  BUG_FIX_ACTION_CAPABILITIES,
  BUG_FIX_CAPABILITIES,
  BUG_FIX_STAGE_CAPABILITIES,
  PENDING_CAPABILITIES,
  loadBugFixManifest,
  registerBugFixCapabilities,
  registerBugFixRegistries,
} from "./bug-fix.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

beforeEach(() => {
  _clearIntakeRegistry();
  registerAll();
});

function makeDb(): Db {
  const tmp = mkdtempSync(join(tmpdir(), "bugfix-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

function makeStubLLM(): StubLLMProvider {
  // Match prefix used by the report intake adapter prompt.
  return new StubLLMProvider(
    new Map([
      [
        "Bug report",
        JSON.stringify({
          summary: "Division crashes when denominator is 0.",
          failureDescription: "Division-by-zero in calculate.",
          fixHint: "Guard before dividing.",
          codeReferences: [
            { file: "src/math.ts", line: 42, function: "calculate" },
          ],
          bugClassHint: "divide-by-zero",
        }),
      ],
    ]),
  );
}

describe("loadBugFixManifest", () => {
  it("parses and structurally validates the YAML manifest on disk", () => {
    const manifest = loadBugFixManifest();
    expect(manifest.name).toBe("bug-fix");
    expect(manifest.cid).toMatch(/bafy-bugfix-/);
    // Output must terminate at bundle.
    expect(manifest.output).toBe("$node.bundle.output");
  });

  it("references every documented bug-fix stage + action capability", () => {
    const manifest = loadBugFixManifest();
    const stageCaps = new Set(manifest.nodes.map((n) => n.capability));
    for (const cap of BUG_FIX_STAGE_CAPABILITIES) {
      expect(stageCaps.has(cap)).toBe(true);
    }
    expect(stageCaps.size).toBe(BUG_FIX_STAGE_CAPABILITIES.length);

    const actionCaps = new Set(
      (manifest.actions ?? []).map((a) => a.action),
    );
    for (const cap of BUG_FIX_ACTION_CAPABILITIES) {
      expect(actionCaps.has(cap)).toBe(true);
    }
    expect(actionCaps.size).toBe(BUG_FIX_ACTION_CAPABILITIES.length);
  });
});

describe("registerBugFixRegistries", () => {
  it("registers every stage capability the manifest names", () => {
    const db = makeDb();
    const { registry } = registerBugFixRegistries({
      db,
      llm: makeStubLLM(),
    });
    const registered = new Set(registry.capabilities());

    for (const cap of BUG_FIX_STAGE_CAPABILITIES) {
      expect(registered.has(cap)).toBe(true);
    }
  });

  it("registers every action capability the manifest names", () => {
    const db = makeDb();
    const { actionRegistry } = registerBugFixRegistries({
      db,
      llm: makeStubLLM(),
    });
    const registered = new Set(actionRegistry.capabilities());

    for (const cap of BUG_FIX_ACTION_CAPABILITIES) {
      expect(registered.has(cap)).toBe(true);
    }
  });

  it("PENDING_CAPABILITIES is empty — the wiring is complete", () => {
    expect(PENDING_CAPABILITIES).toEqual([]);
    // BUG_FIX_CAPABILITIES is the union of stage + action capability
    // names (back-compat with the prior flat shape of this module).
    expect(BUG_FIX_CAPABILITIES.length).toBe(
      BUG_FIX_STAGE_CAPABILITIES.length + BUG_FIX_ACTION_CAPABILITIES.length,
    );
  });

  it("registerBugFixCapabilities returns a stage registry that omits actions", () => {
    // Back-compat shim: registerBugFixCapabilities returns ProducerRegistry.
    // Action capabilities are NOT in the stage registry, by construction.
    const db = makeDb();
    const stageRegistry = registerBugFixCapabilities({
      db,
      llm: makeStubLLM(),
    });
    const registered = new Set(stageRegistry.capabilities());
    for (const cap of BUG_FIX_ACTION_CAPABILITIES) {
      expect(registered.has(cap)).toBe(false);
    }
  });
});

describe("runManifest dispatch", () => {
  it("dispatches a manifest restricted to the registered subset", async () => {
    // Synthetic manifest with only `intake`. Proves the dispatch surface
    // works end-to-end with the bug-fix registry.
    const db = makeDb();
    const llm = makeStubLLM();
    const { registry } = registerBugFixRegistries({ db, llm });
    const { parseManifest } = await import("../workflow/manifest.js");
    const yaml = `
name: bug-fix-intake-only
cid: wf-bugfix-intake-only-v1
nodes:
  - id: intake
    capability: intake
    input:
      text: $input.text
      source: $input.source
output: $node.intake.output
`;
    const manifest = parseManifest(yaml);
    const runner = new WorkflowRunner(
      db,
      manifestToWorkflow(manifest),
      registry,
    );

    const result = await runManifest(runner, registry, manifest, {
      text: "Division crashes when denominator is 0.",
      source: "report",
    });

    const signal = result.output as {
      summary: string;
      codeReferences: unknown[];
    };
    expect(signal.summary).toMatch(/Division crashes/);
    expect(signal.codeReferences).toHaveLength(1);
  });

  it("on the full bug-fix manifest, every named capability resolves (no 'not registered' errors at dispatch)", async () => {
    // The full manifest declares 10 stages + 1 action. Until all
    // producers were wired, this call tripped the runner's pre-flight
    // capability check with /not registered/. With the wiring complete,
    // the pre-flight passes; the call proceeds into actual stage
    // execution. We don't assert the run succeeds end-to-end here (that
    // requires a real fixture; see the integration smoke). We assert
    // only that the failure mode, if any, is NOT a "not registered"
    // pre-flight error.
    const db = makeDb();
    const llm = makeStubLLM();
    const manifest = loadBugFixManifest();
    const { registry, actionRegistry } = registerBugFixRegistries({ db, llm });
    const runner = new WorkflowRunner(
      db,
      manifestToWorkflow(manifest),
      registry,
    );

    let caught: unknown = null;
    try {
      await runManifest(
        runner,
        registry,
        manifest,
        {
          text: "Division crashes when denominator is 0.",
          source: "report",
          projectRoot: "/tmp/does-not-matter",
        },
        actionRegistry,
      );
    } catch (err) {
      caught = err;
    }
    // Whatever failure happens (and one likely does — we passed a
    // bogus projectRoot), it must NOT be the runner's pre-flight
    // "capability X not registered" error.
    if (caught) {
      expect(String(caught)).not.toMatch(/not registered/);
    }
  });
});
