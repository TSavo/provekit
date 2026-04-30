/**
 * mint-migration-plan action tests. The Action writes a markdown file
 * to disk; tests assert the path is correct, the file exists, and the
 * markdown contains the expected sections + counts.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, readFileSync, existsSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryActionRegistry } from "../registry.js";
import {
  makeMintMigrationPlanAction,
  MINT_MIGRATION_PLAN_CAPABILITY,
} from "./mintMigrationPlan.js";
import type { CatalogDeclaration } from "./loadCatalog.js";
import type { DiffCatalogsResult } from "./diffCatalogs.js";
import type { FindImpactedCallsitesResult } from "./findImpactedCallsites.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeProjectAndDb() {
  const projectRoot = mkdtempSync(join(tmpdir(), "mint-plan-"));
  mkdirSync(join(projectRoot, ".provekit"), { recursive: true });
  const db = openDb(join(projectRoot, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return { projectRoot, db };
}

const wf = { name: "test-wf", cid: "wf-mint-plan-test-v1" };

function decl(propertyHash: string, name: string | null = null): CatalogDeclaration {
  return {
    cid: `c-${propertyHash}`,
    propertyHash,
    bindingHash: `b-${propertyHash}`,
    producedBy: "test@1",
    name,
  };
}

const sampleDiff: DiffCatalogsResult = {
  oldFound: true,
  newFound: true,
  oldProofHash: "old123",
  newProofHash: "new456",
  added: [decl("p_added", "newSymbol")],
  removed: [decl("p_removed", "oldSymbol")],
  modified: [
    { name: "renamed", oldPropertyHash: "p_old", newPropertyHash: "p_new" },
  ],
  identical: false,
};

const sampleImpacts: FindImpactedCallsitesResult = {
  impacted: [
    {
      invariantId: "p_removed",
      reason: "removed",
      newPropertyHash: null,
      callsite: {
        filePath: "src/billing/invoice.ts",
        function: "computeTotal",
        startLine: 47,
        endLine: 47,
      },
      originatingBug: "billing total off-by-one",
    },
  ],
  scanned: 5,
  matchStrategy: "propertyHash-collision-v1",
};

describe("mint-migration-plan Action", () => {
  it("writes a plan file at .provekit/migrations/<old>-to-<new>.md", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const action = makeMintMigrationPlanAction();
    const runner = new WorkflowRunner(db, wf);

    const { resource } = await runner.runAction(action, {
      projectRoot,
      diff: sampleDiff,
      impacts: sampleImpacts,
    });

    const expected = join(
      projectRoot,
      ".provekit",
      "migrations",
      "old123-to-new456.md",
    );
    expect(resource.planPath).toBe(expected);
    expect(existsSync(expected)).toBe(true);
  });

  it("captures inline counts on the resource", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const action = makeMintMigrationPlanAction();
    const runner = new WorkflowRunner(db, wf);

    const { resource } = await runner.runAction(action, {
      projectRoot,
      diff: sampleDiff,
      impacts: sampleImpacts,
    });

    expect(resource.counts).toEqual({
      added: 1,
      removed: 1,
      modified: 1,
      impactedCallsites: 1,
    });
  });

  it("writes markdown with Added, Removed, Modified, and Impacted callsites sections", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const action = makeMintMigrationPlanAction();
    const runner = new WorkflowRunner(db, wf);

    const { resource } = await runner.runAction(action, {
      projectRoot,
      diff: sampleDiff,
      impacts: sampleImpacts,
    });

    const md = readFileSync(resource.planPath, "utf-8");
    expect(md).toContain("# Migration plan: old123 → new456");
    expect(md).toContain("## Added (1)");
    expect(md).toContain("## Removed (1)");
    expect(md).toContain("## Modified (1)");
    expect(md).toContain("## Impacted callsites (1)");
    expect(md).toContain("p_added");
    expect(md).toContain("p_removed");
    expect(md).toContain("renamed");
    expect(md).toContain("src/billing/invoice.ts:47");
    expect(md).toContain("propertyHash-collision-v1");
  });

  it("writes a no-op message when the diff is identical", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const action = makeMintMigrationPlanAction();
    const runner = new WorkflowRunner(db, wf);

    const identicalDiff: DiffCatalogsResult = {
      ...sampleDiff,
      added: [],
      removed: [],
      modified: [],
      identical: true,
    };
    const noImpacts: FindImpactedCallsitesResult = {
      impacted: [],
      scanned: 0,
      matchStrategy: "propertyHash-collision-v1",
    };

    const { resource } = await runner.runAction(action, {
      projectRoot,
      diff: identicalDiff,
      impacts: noImpacts,
    });

    const md = readFileSync(resource.planPath, "utf-8");
    expect(md).toContain("No contract changes between these two proofHashes");
  });

  it("produces a fresh audit memento each invocation (no cache reuse)", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const action = makeMintMigrationPlanAction();
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runAction(action, {
      projectRoot,
      diff: sampleDiff,
      impacts: sampleImpacts,
    });
    const b = await runner.runAction(action, {
      projectRoot,
      diff: sampleDiff,
      impacts: sampleImpacts,
    });

    expect(b.auditCid).not.toBe(a.auditCid);
  });

  it("dispatches via the registry as capability 'mint-migration-plan'", async () => {
    const { projectRoot, db } = makeProjectAndDb();
    const action = makeMintMigrationPlanAction();
    const registry = new InMemoryActionRegistry();
    registry.register(MINT_MIGRATION_PLAN_CAPABILITY, action);
    const runner = new WorkflowRunner(db, wf);

    const resolved = registry.resolve(MINT_MIGRATION_PLAN_CAPABILITY);
    expect(resolved).not.toBeNull();
    const { resource } = await runner.runAction(resolved!, {
      projectRoot,
      diff: sampleDiff,
      impacts: sampleImpacts,
    } as unknown as Parameters<typeof runner.runAction>[1]);
    expect((resource as { counts: { added: number } }).counts.added).toBe(1);
  });
});
