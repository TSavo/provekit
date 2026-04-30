/**
 * Explain workflow integration test. Drives the YAML manifest end-to-end
 * via runManifest, asserting:
 * 1. The manifest loads cleanly.
 * 2. A locally-rooted DAG renders correctly.
 * 3. Unresolved inputCids surface in the workflow output (scope check).
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
  loadExplainManifest,
  registerExplainRegistries,
  EXPLAIN_STAGE_CAPABILITIES,
  EXPLAIN_ACTION_CAPABILITIES,
  type ExplainWorkflowInput,
} from "./explain.js";
import type { RenderProofChainOutput } from "../workflow/producers/renderProofChain.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "explain-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("explain workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadExplainManifest();
    expect(manifest.name).toBe("explain");
    expect(manifest.nodes).toHaveLength(1);
    expect(manifest.nodes[0].capability).toBe("render-proof-chain");
    expect(manifest.actions ?? []).toHaveLength(0);
  });

  it("renders a locally-rooted DAG via runManifest", async () => {
    const db = makeDb();
    const leaf = writeMemento(db, {
      bindingHash: "bh-leaf",
      propertyHash: "ph-leaf",
      verdict: "holds",
      witness: "leaf",
      producedBy: "test-v1",
    });
    const root = writeMemento(db, {
      bindingHash: "bh-root",
      propertyHash: "ph-root",
      verdict: "holds",
      witness: "root",
      producedBy: "test-v1",
      inputCids: [leaf.cid!],
    });

    const manifest = loadExplainManifest();
    const { registry } = registerExplainRegistries({ db });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: ExplainWorkflowInput = { startCid: root.cid! };
    const result = await runManifest(runner, registry, manifest, input);

    const output = result.output as RenderProofChainOutput;
    expect(output.startResolved).toBe(true);
    expect(output.mementos).toHaveLength(2);
    expect(output.mementos[0].cid).toBe(root.cid);
    expect(output.mementos[1].cid).toBe(leaf.cid);
    expect(output.unresolvedInputCids).toEqual([]);
  });

  it("surfaces unresolved inputCids without external walking", async () => {
    const db = makeDb();
    const m = writeMemento(db, {
      bindingHash: "bh-only",
      propertyHash: "ph-only",
      verdict: "holds",
      witness: "x",
      producedBy: "test-v1",
      inputCids: ["external-cid-not-here"],
    });

    const manifest = loadExplainManifest();
    const { registry } = registerExplainRegistries({ db });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const result = await runManifest(runner, registry, manifest, {
      startCid: m.cid!,
    });

    const output = result.output as RenderProofChainOutput;
    expect(output.unresolvedInputCids).toEqual(["external-cid-not-here"]);
  });

  it("declares the expected capabilities", () => {
    expect(EXPLAIN_STAGE_CAPABILITIES).toEqual(["render-proof-chain"]);
    expect(EXPLAIN_ACTION_CAPABILITIES).toEqual([]);
  });
});
