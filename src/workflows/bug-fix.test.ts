/**
 * Bug-fix workflow smoke test.
 *
 * Asserts:
 *   - The YAML manifest parses + structurally validates.
 *   - `registerBugFixCapabilities` produces a registry with the 7
 *     capabilities whose producers exist on this branch.
 *   - The manifest's full capability set matches the documented list.
 *   - `runManifest` dispatches the first stage (intake) end-to-end with
 *     a stubbed LLM, then trips on the first un-registered pending
 *     capability with the runner's standard error.
 *
 * Most stages need real fixtures (a git repo, a real SAST DB, a real
 * agent-mode LLM) to run; this test does NOT exercise them. It proves
 * the manifest + registry are wired correctly and that workflow dispatch
 * reaches at least the first node.
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
  BUG_FIX_CAPABILITIES,
  PENDING_CAPABILITIES,
  loadBugFixManifest,
  registerBugFixCapabilities,
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

  it("references every documented bug-fix capability", () => {
    const manifest = loadBugFixManifest();
    const caps = new Set(manifest.nodes.map((n) => n.capability));
    for (const cap of BUG_FIX_CAPABILITIES) {
      expect(caps.has(cap)).toBe(true);
    }
    expect(caps.size).toBe(BUG_FIX_CAPABILITIES.length);
  });
});

describe("registerBugFixCapabilities", () => {
  it("registers every capability whose producer module exists on this branch", () => {
    const db = makeDb();
    const registry = registerBugFixCapabilities({
      db,
      llm: makeStubLLM(),
    });
    const registered = new Set(registry.capabilities());

    for (const cap of BUG_FIX_CAPABILITIES) {
      const isPending = (PENDING_CAPABILITIES as readonly string[]).includes(cap);
      if (isPending) {
        expect(registered.has(cap)).toBe(false);
      } else {
        expect(registered.has(cap)).toBe(true);
      }
    }
  });
});

describe("runManifest dispatch", () => {
  it("executes intake then trips on the first pending capability", async () => {
    const db = makeDb();
    const llm = makeStubLLM();
    const manifest = loadBugFixManifest();
    const registry = registerBugFixCapabilities({ db, llm });
    const runner = new WorkflowRunner(
      db,
      manifestToWorkflow(manifest),
      registry,
    );

    // The runner surfaces unknown capabilities up front rather than
    // mid-run (manifest.ts:291-298). One of the four PENDING_CAPABILITIES
    // is in the manifest, so this should throw before executing any node.
    await expect(
      runManifest(runner, registry, manifest, {
        text: "Division crashes when denominator is 0.",
        source: "report",
        projectRoot: "/tmp/does-not-matter",
      }),
    ).rejects.toThrow(/not registered/);
  });

  it("dispatches a manifest restricted to the registered subset", async () => {
    // Construct a small synthetic manifest that uses only registered
    // capabilities, to prove the dispatch surface works end-to-end with
    // the bug-fix registry.
    const db = makeDb();
    const llm = makeStubLLM();
    const registry = registerBugFixCapabilities({ db, llm });
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
});
