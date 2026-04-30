/**
 * Hook workflow end-to-end smoke. Asserts the manifest parses, the
 * Stage + Action register, and runManifest dispatches them in the
 * right order.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb, type Db } from "../db/index.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import {
  loadHookManifest,
  registerHookRegistries,
  HOOK_STAGE_CAPABILITIES,
  HOOK_ACTION_CAPABILITIES,
  type HookWorkflowInput,
} from "./hook.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb(): Db {
  const tmp = mkdtempSync(join(tmpdir(), "hook-workflow-test-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("hook workflow manifest", () => {
  it("parses with the cli: block", () => {
    const m = loadHookManifest();
    expect(m.name).toBe("hook");
    expect(m.cli).toBeDefined();
    expect(m.cli!.args!.find((a) => a.name === "uninstall")?.flag).toBe(true);
    expect(m.cli!.args!.find((a) => a.name === "status")?.flag).toBe(true);
  });

  it("declares the expected stage and action capabilities", () => {
    expect(HOOK_STAGE_CAPABILITIES).toEqual(["plan-hook-operation"]);
    expect(HOOK_ACTION_CAPABILITIES).toEqual(["manage-git-hook"]);
  });
});

describe("hook workflow runManifest", () => {
  it("plans + executes a status query in a non-git tmp dir", async () => {
    const db = makeDb();
    const manifest = loadHookManifest();
    const { registry, actionRegistry } = registerHookRegistries();
    const runner = new WorkflowRunner(
      db,
      manifestToWorkflow(manifest),
      registry,
    );

    const tmp = mkdtempSync(join(tmpdir(), "hook-status-"));
    const input: HookWorkflowInput = { operation: "status", projectRoot: tmp };
    const result = await runManifest(
      runner,
      registry,
      manifest,
      input,
      actionRegistry,
    );

    expect(result.output).toEqual({ operation: "status", projectRoot: tmp });
  });

  it("rejects unknown operations at plan time", async () => {
    const db = makeDb();
    const manifest = loadHookManifest();
    const { registry, actionRegistry } = registerHookRegistries();
    const runner = new WorkflowRunner(
      db,
      manifestToWorkflow(manifest),
      registry,
    );

    await expect(
      runManifest(
        runner,
        registry,
        manifest,
        { operation: "burn" as unknown as HookWorkflowInput["operation"], projectRoot: "/tmp" },
        actionRegistry,
      ),
    ).rejects.toThrow(/unknown operation/);
  });
});
