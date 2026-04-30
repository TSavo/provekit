/**
 * Migrate workflow integration test. Drives the YAML manifest end-to-end
 * via runManifest, asserting:
 *  1. The manifest loads cleanly.
 *  2. Stages compose correctly when both catalogs exist in the local DB.
 *  3. The Action lands a markdown plan on disk.
 *  4. found=false on either side flows through to the plan preamble.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, existsSync, readFileSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate as drizzleMigrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { writeMemento } from "../fix/runtime/mementoStore.js";
import { writeInvariant, type StoredInvariant } from "../fix/runtime/invariantStore.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import {
  loadMigrateManifest,
  registerMigrateRegistries,
  MIGRATE_STAGE_CAPABILITIES,
  MIGRATE_ACTION_CAPABILITIES,
  type MigrateWorkflowInput,
} from "./migrate.js";
import type { FindImpactedCallsitesResult } from "../workflow/producers/findImpactedCallsites.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeProjectAndDb() {
  const projectRoot = mkdtempSync(join(tmpdir(), "migrate-wf-"));
  mkdirSync(join(projectRoot, ".provekit"), { recursive: true });
  const db = openDb(join(projectRoot, ".provekit", "test.db"));
  drizzleMigrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return { projectRoot, db };
}

function writeChild(
  db: ReturnType<typeof makeProjectAndDb>["db"],
  bindingHash: string,
  propertyHash: string,
) {
  return writeMemento(db, {
    bindingHash,
    propertyHash,
    verdict: "holds",
    witness: "child",
    producedBy: "test-kit@1.0",
  });
}

function writeCatalog(
  db: ReturnType<typeof makeProjectAndDb>["db"],
  bindingHash: string,
  propertyHash: string,
  inputCids: string[],
) {
  return writeMemento(db, {
    bindingHash,
    propertyHash,
    verdict: "holds",
    witness: "catalog",
    producedBy: "test-kit@1.0",
    inputCids,
  });
}

function makeInv(id: string): StoredInvariant {
  return {
    id,
    createdAt: "2026-04-29T00:00:00.000Z",
    originatingBug: id,
    smt: {
      kind: "arithmetic",
      declarations: ["(declare-const x Int)"],
      assertion: "(assert (not (= x 0)))",
    },
    bindings: [
      {
        type: "local",
        smt_constant: "x",
        source_expr: "expr",
        sort: "Int",
        node: {
          filePath: "src/m.ts",
          nodeHash: "h",
          startLine: 1,
          endLine: 1,
        },
      },
    ],
    callsite: {
      filePath: "src/m.ts",
      function: null,
      startLine: 1,
      endLine: 1,
    },
    scope: "callsite",
    regressionTest: null,
    patchSha: null,
    retired: null,
  };
}

describe("migrate workflow", () => {
  it("loads the manifest cleanly with three stages and one action", () => {
    const manifest = loadMigrateManifest();
    expect(manifest.name).toBe("migrate");
    expect(manifest.actions ?? []).toHaveLength(1);
    expect((manifest.actions ?? [])[0].action).toBe(
      MIGRATE_ACTION_CAPABILITIES[0],
    );
    expect(manifest.output).toBe("$node.find-impacts.output");
    // load-catalog appears twice in the manifest as separate node ids.
    const capabilities = new Set(manifest.nodes.map((n) => n.capability));
    expect(capabilities).toEqual(new Set(MIGRATE_STAGE_CAPABILITIES));
  });

  it("end-to-end: emits a plan flagging an impacted invariant", async () => {
    const { projectRoot, db } = makeProjectAndDb();

    // Old catalog: pa survives; pb gets removed.
    const childA1 = writeChild(db, "ba", "pa");
    const childB = writeChild(db, "bb", "pb");
    const oldCatalog = writeCatalog(db, "co", "po", [childA1.cid!, childB.cid!]);

    // New catalog: pa survives, pc is added (pb was removed).
    const childA2 = writeChild(db, "ba2", "pa");
    const childC = writeChild(db, "bc", "pc");
    const newCatalog = writeCatalog(db, "cn", "pn", [childA2.cid!, childC.cid!]);

    // Project invariant whose id collides with the removed propertyHash.
    writeInvariant(projectRoot, makeInv("pb"));

    const manifest = loadMigrateManifest();
    const { registry, actionRegistry } = registerMigrateRegistries({ db });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: MigrateWorkflowInput = {
      projectRoot,
      oldProofHash: oldCatalog.cid!,
      newProofHash: newCatalog.cid!,
    };
    const result = await runManifest(
      runner,
      registry,
      manifest,
      input,
      actionRegistry,
    );

    const impacts = result.output as FindImpactedCallsitesResult;
    expect(impacts.scanned).toBe(1);
    expect(impacts.impacted).toHaveLength(1);
    expect(impacts.impacted[0].invariantId).toBe("pb");
    expect(impacts.impacted[0].reason).toBe("removed");

    const planPath = join(
      projectRoot,
      ".provekit",
      "migrations",
      `${oldCatalog.cid}-to-${newCatalog.cid}.md`,
    );
    expect(existsSync(planPath)).toBe(true);
    const md = readFileSync(planPath, "utf-8");
    expect(md).toContain("Removed (1)");
    expect(md).toContain("pb");
    expect(md).toContain("propertyHash-collision-v1");
  });

  it("flags found=false when a proofHash is not in the local store", async () => {
    const { projectRoot, db } = makeProjectAndDb();

    const child = writeChild(db, "ba", "pa");
    const newCatalog = writeCatalog(db, "cn", "pn", [child.cid!]);

    const manifest = loadMigrateManifest();
    const { registry, actionRegistry } = registerMigrateRegistries({ db });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: MigrateWorkflowInput = {
      projectRoot,
      oldProofHash: "deadbeef".repeat(4),
      newProofHash: newCatalog.cid!,
    };
    const result = await runManifest(
      runner,
      registry,
      manifest,
      input,
      actionRegistry,
    );

    const impacts = result.output as FindImpactedCallsitesResult;
    expect(impacts.impacted).toEqual([]);

    const planPath = join(
      projectRoot,
      ".provekit",
      "migrations",
      `${"deadbeef".repeat(4)}-to-${newCatalog.cid}.md`,
    );
    const md = readFileSync(planPath, "utf-8");
    expect(md).toContain("at least one catalog memento was not found");
  });
});
