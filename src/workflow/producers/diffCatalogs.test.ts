/**
 * diff-catalogs stage tests. Pure data transformation; no DB
 * required for the algorithm, but the runner persists mementos so a
 * DB is needed.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeDiffCatalogsStage,
  DIFF_CATALOGS_CAPABILITY,
  type DiffCatalogsResult,
  type DiffCatalogsStageInput,
} from "./diffCatalogs.js";
import type {
  CatalogDeclaration,
  LoadCatalogResult,
} from "./loadCatalog.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "diff-catalogs-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

function decl(
  cid: string,
  propertyHash: string,
  name: string | null = null,
): CatalogDeclaration {
  return {
    cid,
    propertyHash,
    bindingHash: `b-${cid}`,
    producedBy: "test@1",
    name,
  };
}

function catalog(proofHash: string, declarations: CatalogDeclaration[]): LoadCatalogResult {
  return {
    found: true,
    proofHash,
    producedBy: "test@1",
    declarations,
  };
}

const wf = { name: "test-wf", cid: "wf-diff-catalogs-test-v1" };

describe("diff-catalogs Stage", () => {
  it("returns identical=true when catalogs share the same propertyHashes", async () => {
    const db = makeDb();
    const old = catalog("old", [decl("c1", "p1"), decl("c2", "p2")]);
    const fresh = catalog("new", [decl("c3", "p1"), decl("c4", "p2")]);
    const stage = makeDiffCatalogsStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      oldCatalog: old,
      newCatalog: fresh,
    });

    expect(output.identical).toBe(true);
    expect(output.added).toEqual([]);
    expect(output.removed).toEqual([]);
    expect(output.modified).toEqual([]);
  });

  it("computes Added and Removed when names are absent", async () => {
    const db = makeDb();
    const old = catalog("old", [decl("c1", "p1"), decl("c2", "p2")]);
    const fresh = catalog("new", [decl("c1", "p1"), decl("c3", "p3")]);
    const stage = makeDiffCatalogsStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      oldCatalog: old,
      newCatalog: fresh,
    });

    expect(output.identical).toBe(false);
    expect(output.added.map((d) => d.propertyHash)).toEqual(["p3"]);
    expect(output.removed.map((d) => d.propertyHash)).toEqual(["p2"]);
    expect(output.modified).toEqual([]);
  });

  it("computes Modified when the same name maps to a new propertyHash", async () => {
    const db = makeDb();
    const old = catalog("old", [decl("c1", "p1", "parseInt"), decl("c2", "p2")]);
    const fresh = catalog("new", [decl("c3", "p1prime", "parseInt"), decl("c2", "p2")]);
    const stage = makeDiffCatalogsStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      oldCatalog: old,
      newCatalog: fresh,
    });

    expect(output.modified).toHaveLength(1);
    expect(output.modified[0]).toEqual({
      name: "parseInt",
      oldPropertyHash: "p1",
      newPropertyHash: "p1prime",
    });
    // Modified declarations are stripped from added/removed.
    expect(output.added).toEqual([]);
    expect(output.removed).toEqual([]);
  });

  it("treats null-name declarations as add/remove only, never modified", async () => {
    const db = makeDb();
    const old = catalog("old", [decl("c1", "p1")]);
    const fresh = catalog("new", [decl("c2", "p2")]);
    const stage = makeDiffCatalogsStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      oldCatalog: old,
      newCatalog: fresh,
    });

    expect(output.modified).toEqual([]);
    expect(output.added.map((d) => d.propertyHash)).toEqual(["p2"]);
    expect(output.removed.map((d) => d.propertyHash)).toEqual(["p1"]);
  });

  it("handles found=false on either side as an empty declaration set", async () => {
    const db = makeDb();
    const old: LoadCatalogResult = { found: false, proofHash: "old" };
    const fresh = catalog("new", [decl("c1", "p1", "parseInt")]);
    const stage = makeDiffCatalogsStage();
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      oldCatalog: old,
      newCatalog: fresh,
    });

    expect(output.oldFound).toBe(false);
    expect(output.newFound).toBe(true);
    expect(output.added).toHaveLength(1);
    expect(output.removed).toEqual([]);
    expect(output.modified).toEqual([]);
  });

  it("caches identical input — second run is a hit", async () => {
    const db = makeDb();
    const old = catalog("old", [decl("c1", "p1")]);
    const fresh = catalog("new", [decl("c2", "p2")]);
    const stage = makeDiffCatalogsStage();
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { oldCatalog: old, newCatalog: fresh });
    const b = await runner.runStage(stage, { oldCatalog: old, newCatalog: fresh });

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
  });

  it("dispatches via the registry as capability 'diff-catalogs'", async () => {
    const db = makeDb();
    const stage = makeDiffCatalogsStage();
    const registry = new InMemoryRegistry();
    registry.register(DIFF_CATALOGS_CAPABILITY, stage);
    const runner = new WorkflowRunner(db, wf, registry);

    const result = await runner.request<DiffCatalogsStageInput, DiffCatalogsResult>(
      DIFF_CATALOGS_CAPABILITY,
      {
        oldCatalog: catalog("old", []),
        newCatalog: catalog("new", []),
      },
    );

    expect(result.output.identical).toBe(true);
  });
});
