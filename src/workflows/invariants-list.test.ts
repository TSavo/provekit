/**
 * Invariants-list workflow integration test. Verifies:
 * 1. Manifest loads cleanly with one Stage and zero Actions.
 * 2. End-to-end runManifest returns empty list when store doesn't exist.
 * 3. Capability declarations match what the manifest references.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import {
  loadInvariantsListManifest,
  registerInvariantsListRegistries,
  INVARIANTS_LIST_STAGE_CAPABILITIES,
  INVARIANTS_LIST_ACTION_CAPABILITIES,
  type InvariantsListWorkflowInput,
} from "./invariants-list.js";
import type { ListInvariantsOutput } from "../workflow/producers/listInvariants.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "inv-list-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("invariants-list workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadInvariantsListManifest();
    expect(manifest.name).toBe("invariants-list");
    expect(manifest.nodes).toHaveLength(1);
    expect(manifest.nodes[0].capability).toBe("list-invariants");
    expect(manifest.actions).toHaveLength(0);
  });

  it("declares a CLI block consumed by the meta-dispatcher", () => {
    const manifest = loadInvariantsListManifest();
    expect(manifest.cli).toBeDefined();
    const argNames = manifest.cli!.args!.map((a) => a.name);
    expect(argNames).toContain("projectRoot");
    expect(argNames).toContain("all");
  });

  it("returns empty list with storeExists=false when no .provekit/invariants/", async () => {
    const db = makeDb();
    const projectRoot = mkdtempSync(join(tmpdir(), "inv-list-project-"));

    const manifest = loadInvariantsListManifest();
    const { registry, actionRegistry } = registerInvariantsListRegistries();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: InvariantsListWorkflowInput = {
      projectRoot,
      includeRetired: false,
    };
    const result = await runManifest(runner, registry, manifest, input, actionRegistry);
    const out = result.output as ListInvariantsOutput;

    expect(out.storeExists).toBe(false);
    expect(out.invariants).toEqual([]);
  });

  it("declares the expected capabilities", () => {
    expect(INVARIANTS_LIST_STAGE_CAPABILITIES).toEqual(["list-invariants"]);
    expect(INVARIANTS_LIST_ACTION_CAPABILITIES).toEqual([]);
  });
});
