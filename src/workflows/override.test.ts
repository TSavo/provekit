/**
 * Override workflow end-to-end smoke. Validates the manifest parses,
 * the registry registers the single capability, and runManifest
 * produces the expected override-record output.
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
  loadOverrideManifest,
  registerOverrideRegistries,
  OVERRIDE_STAGE_CAPABILITIES,
  OVERRIDE_ACTION_CAPABILITIES,
  type OverrideWorkflowInput,
} from "./override.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb(): Db {
  const tmp = mkdtempSync(join(tmpdir(), "override-workflow-test-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("override workflow manifest", () => {
  it("parses with cli: block intact", () => {
    const m = loadOverrideManifest();
    expect(m.name).toBe("override");
    expect(m.cli).toBeDefined();
    expect(m.cli!.description).toMatch(/override/i);
    expect(m.cli!.args).toBeDefined();
    const reasonArg = m.cli!.args!.find((a) => a.name === "reason");
    expect(reasonArg).toBeDefined();
    expect(reasonArg!.required).toBe(true);
  });

  it("declares the expected stage and zero actions", () => {
    expect(OVERRIDE_STAGE_CAPABILITIES).toEqual(["record-override"]);
    expect(OVERRIDE_ACTION_CAPABILITIES).toHaveLength(0);
  });
});

describe("override workflow runManifest", () => {
  it("produces an override-record from a reason", async () => {
    const db = makeDb();
    const manifest = loadOverrideManifest();
    const { registry, actionRegistry } = registerOverrideRegistries();
    const runner = new WorkflowRunner(
      db,
      manifestToWorkflow(manifest),
      registry,
    );

    const input: OverrideWorkflowInput = { reason: "intentional refactor gap" };
    const result = await runManifest(
      runner,
      registry,
      manifest,
      input,
      actionRegistry,
    );

    expect(result.output).toMatchObject({
      reason: "intentional refactor gap",
      followupCommand: "git commit --no-verify",
    });
    expect((result.output as { message: string }).message).toContain(
      "intentional refactor gap",
    );
    expect(result.cid).toBeTruthy();
  });

  it("hits the workflow-level cache on a second identical call", async () => {
    const db = makeDb();
    const manifest = loadOverrideManifest();
    const { registry, actionRegistry } = registerOverrideRegistries();
    const runner = new WorkflowRunner(
      db,
      manifestToWorkflow(manifest),
      registry,
    );

    const input: OverrideWorkflowInput = { reason: "same reason twice" };
    await runManifest(runner, registry, manifest, input, actionRegistry);
    const second = await runManifest(
      runner,
      registry,
      manifest,
      input,
      actionRegistry,
    );
    expect(second.cacheHit).toBe(true);
  });

  it("rejects an empty reason at the stage level", async () => {
    const db = makeDb();
    const manifest = loadOverrideManifest();
    const { registry, actionRegistry } = registerOverrideRegistries();
    const runner = new WorkflowRunner(
      db,
      manifestToWorkflow(manifest),
      registry,
    );

    await expect(
      runManifest(runner, registry, manifest, { reason: "" }, actionRegistry),
    ).rejects.toThrow(/non-empty reason/);
  });
});
