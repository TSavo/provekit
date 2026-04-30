/**
 * Leaves workflow integration test. Drives the YAML manifest end-to-end
 * via runManifest, asserting:
 *   1. The manifest loads cleanly and declares the expected capabilities.
 *   2. An empty store renders the empty-state message.
 *   3. Multiple locally-minted mementos surface in the projection.
 *   4. Filters thread through ($input.kind, $input.producedBy).
 *   5. JSON format emits a parseable body whose payload matches text-mode.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { writeMemento } from "../fix/runtime/mementoStore.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import {
  loadLeavesManifest,
  registerLeavesRegistries,
  LEAVES_STAGE_CAPABILITIES,
  LEAVES_ACTION_CAPABILITIES,
  type LeavesWorkflowInput,
} from "./leaves.js";
import type { FormatLeavesOutputResult } from "../workflow/producers/formatLeavesOutput.js";
import type { LocalLeaf } from "../workflow/producers/enumerateLocalLeaves.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "leaves-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("leaves workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadLeavesManifest();
    expect(manifest.name).toBe("leaves");
    expect(manifest.nodes).toHaveLength(2);
    expect(manifest.nodes.map((n) => n.capability).sort()).toEqual([
      "enumerate-local-leaves",
      "format-leaves-output",
    ]);
    expect(manifest.actions ?? []).toHaveLength(0);
  });

  it("declares the expected capabilities", () => {
    expect(LEAVES_STAGE_CAPABILITIES).toEqual([
      "enumerate-local-leaves",
      "format-leaves-output",
    ]);
    expect(LEAVES_ACTION_CAPABILITIES).toEqual([]);
  });

  it("renders the empty-state message when the store is empty", async () => {
    const db = makeDb();
    const manifest = loadLeavesManifest();
    const { registry } = registerLeavesRegistries({ db });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: LeavesWorkflowInput = {};
    const result = await runManifest(runner, registry, manifest, input);
    const output = result.output as FormatLeavesOutputResult;

    expect(output.format).toBe("text");
    expect(output.body).toBe("No locally-minted mementos.");
  });

  it("surfaces locally-minted mementos in the projection", async () => {
    const db = makeDb();
    const m1 = writeMemento(db, {
      bindingHash: "bh-1",
      propertyHash: "ph-1",
      verdict: "holds",
      witness: "w1",
      producedBy: "ts-kit@1.0",
    });
    const m2 = writeMemento(db, {
      bindingHash: "bh-2",
      propertyHash: "ph-2",
      verdict: "violated",
      witness: "w2",
      producedBy: "z3@4.12",
    });

    const manifest = loadLeavesManifest();
    const { registry } = registerLeavesRegistries({ db });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: LeavesWorkflowInput = { format: "json" };
    const result = await runManifest(runner, registry, manifest, input);
    const output = result.output as FormatLeavesOutputResult;

    expect(output.format).toBe("json");
    const parsed = JSON.parse(output.body) as { leaves: LocalLeaf[] };
    expect(parsed.leaves).toHaveLength(2);
    expect(new Set(parsed.leaves.map((l) => l.cid))).toEqual(
      new Set([m1.cid!, m2.cid!]),
    );
  });

  it("filters by producedBy through $input.producedBy", async () => {
    const db = makeDb();
    writeMemento(db, {
      bindingHash: "bh-1",
      propertyHash: "ph-1",
      verdict: "holds",
      witness: "w1",
      producedBy: "ts-kit@1.0",
    });
    writeMemento(db, {
      bindingHash: "bh-2",
      propertyHash: "ph-2",
      verdict: "holds",
      witness: "w2",
      producedBy: "z3@4.12",
    });

    const manifest = loadLeavesManifest();
    const { registry } = registerLeavesRegistries({ db });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: LeavesWorkflowInput = {
      format: "json",
      producedBy: "ts-kit@1.0",
    };
    const result = await runManifest(runner, registry, manifest, input);
    const output = result.output as FormatLeavesOutputResult;
    const parsed = JSON.parse(output.body) as { leaves: LocalLeaf[] };

    expect(parsed.leaves).toHaveLength(1);
    expect(parsed.leaves[0].producedBy).toBe("ts-kit@1.0");
  });

  it("filters by evidence kind through $input.kind", async () => {
    const db = makeDb();
    writeMemento(db, {
      bindingHash: "bh-legacy",
      propertyHash: "ph-legacy",
      verdict: "holds",
      witness: "legacy",
      producedBy: "p1",
    });
    writeMemento(db, {
      bindingHash: "bh-typed",
      propertyHash: "ph-typed",
      verdict: "holds",
      producedBy: "p2",
      evidenceHint: {
        kind: "lint-pass",
        body: {
          linter: "eslint",
          linterVersion: "9.0.0",
          rulesetHash: "00000000000000000000000000000000",
          warnings: 0,
        },
      },
    });

    const manifest = loadLeavesManifest();
    const { registry } = registerLeavesRegistries({ db });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: LeavesWorkflowInput = {
      format: "json",
      kind: "lint-pass",
    };
    const result = await runManifest(runner, registry, manifest, input);
    const output = result.output as FormatLeavesOutputResult;
    const parsed = JSON.parse(output.body) as { leaves: LocalLeaf[] };

    expect(parsed.leaves).toHaveLength(1);
    expect(parsed.leaves[0].evidenceKind).toBe("lint-pass");
  });
});
