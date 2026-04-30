/**
 * Roots workflow integration test. Drives the YAML manifest end-to-end
 * via runManifest, asserting:
 *   1. The manifest loads cleanly and declares the expected capabilities.
 *   2. An empty store renders the "no external roots" message.
 *   3. External CIDs surface as roots while locally-minted ones are excluded.
 *   4. JSON format emits a parseable body whose roots array matches the
 *      sorted set difference.
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
  loadRootsManifest,
  registerRootsRegistries,
  ROOTS_STAGE_CAPABILITIES,
  ROOTS_ACTION_CAPABILITIES,
  type RootsWorkflowInput,
} from "./roots.js";
import type { FormatRootsOutputResult } from "../workflow/producers/formatRootsOutput.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "roots-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("roots workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadRootsManifest();
    expect(manifest.name).toBe("roots");
    expect(manifest.nodes).toHaveLength(2);
    expect(manifest.nodes.map((n) => n.capability).sort()).toEqual([
      "enumerate-local-roots",
      "format-roots-output",
    ]);
    expect(manifest.actions ?? []).toHaveLength(0);
  });

  it("declares the expected capabilities", () => {
    expect(ROOTS_STAGE_CAPABILITIES).toEqual([
      "enumerate-local-roots",
      "format-roots-output",
    ]);
    expect(ROOTS_ACTION_CAPABILITIES).toEqual([]);
  });

  it("renders the no-external-roots message when the store is empty", async () => {
    const db = makeDb();
    const manifest = loadRootsManifest();
    const { registry } = registerRootsRegistries({ db });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: RootsWorkflowInput = {};
    const result = await runManifest(runner, registry, manifest, input);
    const output = result.output as FormatRootsOutputResult;

    expect(output.format).toBe("text");
    expect(output.body).toBe(
      "No external roots — every referenced CID was minted locally.",
    );
  });

  it("surfaces external CIDs while excluding locally-minted ones", async () => {
    const db = makeDb();
    const leaf = writeMemento(db, {
      bindingHash: "bh-leaf",
      propertyHash: "ph-leaf",
      verdict: "holds",
      witness: "leaf",
      producedBy: "p",
    });
    writeMemento(db, {
      bindingHash: "bh-root",
      propertyHash: "ph-root",
      verdict: "holds",
      witness: "root",
      producedBy: "p",
      inputCids: [leaf.cid!, "external-zeta", "external-alpha"],
    });

    const manifest = loadRootsManifest();
    const { registry } = registerRootsRegistries({ db });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: RootsWorkflowInput = { format: "json" };
    const result = await runManifest(runner, registry, manifest, input);
    const output = result.output as FormatRootsOutputResult;

    expect(output.format).toBe("json");
    const parsed = JSON.parse(output.body) as { roots: string[] };
    expect(parsed.roots).toEqual(["external-alpha", "external-zeta"]);
  });

  it("renders external roots in the default text format", async () => {
    const db = makeDb();
    writeMemento(db, {
      bindingHash: "bh",
      propertyHash: "ph",
      verdict: "holds",
      witness: "w",
      producedBy: "p",
      inputCids: ["external-cid"],
    });

    const manifest = loadRootsManifest();
    const { registry } = registerRootsRegistries({ db });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const result = await runManifest(runner, registry, manifest, {});
    const output = result.output as FormatRootsOutputResult;

    expect(output.format).toBe("text");
    expect(output.body).toContain("External roots: 1");
    expect(output.body).toContain("external-cid");
  });
});
