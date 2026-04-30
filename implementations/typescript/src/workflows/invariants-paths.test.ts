/**
 * Invariants-paths workflow integration test. Verifies:
 * 1. Manifest loads cleanly with one Stage and zero Actions.
 * 2. End-to-end runManifest throws a useful error when the requested
 *    invariant is not found.
 * 3. Capability declarations match what the manifest references.
 *
 * Successful path enumeration requires a populated substrate
 * (.provekit/provekit.db) plus a stored invariant; that's covered by
 * the substrate's own test suite. This file only covers the wiring.
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
  loadInvariantsPathsManifest,
  registerInvariantsPathsRegistries,
  INVARIANTS_PATHS_STAGE_CAPABILITIES,
  INVARIANTS_PATHS_ACTION_CAPABILITIES,
  type InvariantsPathsWorkflowInput,
} from "./invariants-paths.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "inv-paths-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("invariants-paths workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadInvariantsPathsManifest();
    expect(manifest.name).toBe("invariants-paths");
    expect(manifest.nodes).toHaveLength(1);
    expect(manifest.nodes[0].capability).toBe("enumerate-invariant-paths");
    expect(manifest.actions).toHaveLength(0);
  });

  it("declares a CLI block requiring invariantId positional", () => {
    const manifest = loadInvariantsPathsManifest();
    expect(manifest.cli).toBeDefined();
    const args = manifest.cli!.args!;
    const invariantArg = args.find((a) => a.name === "invariantId");
    expect(invariantArg).toBeDefined();
    expect(invariantArg!.positional).toBe(true);
    expect(invariantArg!.required).toBe(true);
  });

  it("throws a clear error when the invariant is not found", async () => {
    const db = makeDb();
    const projectRoot = mkdtempSync(join(tmpdir(), "inv-paths-project-"));

    const manifest = loadInvariantsPathsManifest();
    const { registry, actionRegistry } = registerInvariantsPathsRegistries();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: InvariantsPathsWorkflowInput = {
      projectRoot,
      invariantId: "nope-not-here",
      maxPaths: 50,
    };
    await expect(
      runManifest(runner, registry, manifest, input, actionRegistry),
    ).rejects.toThrow(/invariant nope-not-here not found/);
  });

  it("declares the expected capabilities", () => {
    expect(INVARIANTS_PATHS_STAGE_CAPABILITIES).toEqual([
      "enumerate-invariant-paths",
    ]);
    expect(INVARIANTS_PATHS_ACTION_CAPABILITIES).toEqual([]);
  });
});
