/**
 * Invariants-verify workflow integration test. Verifies:
 * 1. Manifest loads cleanly with one Stage and zero Actions.
 * 2. End-to-end runManifest returns an empty/zero report against an empty
 *    invariant store (the gate is a no-op when nothing's standing).
 * 3. Capability declarations match what the manifest references.
 *
 * Real Z3-driven verification (with stored invariants + substrate) is
 * exercised by the runtime layer's own test suite; this workflow test
 * only covers the manifest-driven wiring.
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
  loadInvariantsVerifyManifest,
  registerInvariantsVerifyRegistries,
  INVARIANTS_VERIFY_STAGE_CAPABILITIES,
  INVARIANTS_VERIFY_ACTION_CAPABILITIES,
  type InvariantsVerifyWorkflowInput,
} from "./invariants-verify.js";
import type { VerifyInvariantsOutput } from "../workflow/producers/verifyInvariants.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "inv-verify-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("invariants-verify workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadInvariantsVerifyManifest();
    expect(manifest.name).toBe("invariants-verify");
    expect(manifest.nodes).toHaveLength(1);
    expect(manifest.nodes[0].capability).toBe("verify-invariants");
    expect(manifest.actions).toHaveLength(0);
  });

  it("declares a CLI block including ci/adversarial/timeout flags", () => {
    const manifest = loadInvariantsVerifyManifest();
    expect(manifest.cli).toBeDefined();
    const argNames = manifest.cli!.args!.map((a) => a.name);
    expect(argNames).toEqual(
      expect.arrayContaining([
        "projectRoot",
        "timeout",
        "maxPaths",
        "adversarial",
        "ci",
      ]),
    );
  });

  it("returns a zero/empty report for a project with no invariants", async () => {
    const db = makeDb();
    const projectRoot = mkdtempSync(join(tmpdir(), "inv-verify-project-"));

    const manifest = loadInvariantsVerifyManifest();
    const { registry, actionRegistry } = registerInvariantsVerifyRegistries();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: InvariantsVerifyWorkflowInput = {
      projectRoot,
      adversarial: false,
    };
    const result = await runManifest(runner, registry, manifest, input, actionRegistry);
    const out = result.output as VerifyInvariantsOutput;

    expect(out.summary.total).toBe(0);
    expect(out.verdicts).toEqual([]);
    // exitCode 0 when nothing fails — no violations on an empty set.
    expect(out.exitCode).toBe(0);
  });

  it("declares the expected capabilities", () => {
    expect(INVARIANTS_VERIFY_STAGE_CAPABILITIES).toEqual(["verify-invariants"]);
    expect(INVARIANTS_VERIFY_ACTION_CAPABILITIES).toEqual([]);
  });
});
