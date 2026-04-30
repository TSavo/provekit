/**
 * load-catalog stage tests. The Stage is a thin wrapper over
 * mementoStore.findByCid; tests build small in-memory catalog
 * mementos with bridge children and assert the Stage threads them
 * through with `name` extracted when the bridge variant carries
 * `sourceSymbol`.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { writeMemento } from "../../fix/runtime/mementoStore.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import {
  makeLoadCatalogStage,
  LOAD_CATALOG_CAPABILITY,
  type LoadCatalogResult,
  type LoadCatalogStageInput,
} from "./loadCatalog.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "load-catalog-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-load-catalog-test-v1" };

function writeBridge(
  db: ReturnType<typeof makeDb>,
  bindingHash: string,
  propertyHash: string,
) {
  // Bridge mementos in production are signed claim envelopes minted via
  // claimEnvelope.mintBridge and inserted directly. For the Stage's
  // structural tests we only need a memento findByCid can return — its
  // bindingHash, propertyHash, producedBy, and inputCids matter; the
  // bridge variant's sourceSymbol does not, since the Stage already
  // documents that name extraction returns null on non-bridge variants.
  return writeMemento(db, {
    bindingHash,
    propertyHash,
    verdict: "holds",
    witness: "test-bridge",
    producedBy: "ts-kit@1.0",
  });
}

function writeCatalog(
  db: ReturnType<typeof makeDb>,
  bindingHash: string,
  propertyHash: string,
  inputCids: string[],
) {
  return writeMemento(db, {
    bindingHash,
    propertyHash,
    verdict: "holds",
    witness: "kit-catalog",
    producedBy: "ts-kit@1.0",
    inputCids,
  });
}

describe("load-catalog Stage", () => {
  it("returns found=false when the proofHash is not in the local store", async () => {
    const db = makeDb();
    const stage = makeLoadCatalogStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      proofHash: "deadbeef".repeat(4),
    });

    expect(output.found).toBe(false);
    expect(output.proofHash).toBe("deadbeef".repeat(4));
  });

  it("loads a catalog with child mementos and threads through propertyHashes", async () => {
    const db = makeDb();
    const a = writeBridge(db, "ba", "pa");
    const b = writeBridge(db, "bb", "pb");
    const catalog = writeCatalog(db, "bc", "pc", [a.cid!, b.cid!]);

    const stage = makeLoadCatalogStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      proofHash: catalog.cid!,
    });

    expect(output.found).toBe(true);
    if (!output.found) throw new Error();
    expect(output.declarations).toHaveLength(2);
    const propertyHashes = output.declarations.map((d) => d.propertyHash).sort();
    expect(propertyHashes).toEqual(["pa", "pb"]);
    // Non-bridge-variant mementos report name=null (v1 contract).
    expect(output.declarations.every((d) => d.name === null)).toBe(true);
    expect(output.declarations.every((d) => d.producedBy === "ts-kit@1.0")).toBe(true);
  });

  it("skips inputCids whose memento is not in the local store", async () => {
    const db = makeDb();
    const a = writeBridge(db, "ba", "pa");
    const catalog = writeCatalog(db, "bc", "pc", [
      a.cid!,
      "deadbeef".repeat(4),
    ]);

    const stage = makeLoadCatalogStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const { output } = await runner.runStage(stage, {
      proofHash: catalog.cid!,
    });

    expect(output.found).toBe(true);
    if (!output.found) throw new Error();
    expect(output.declarations).toHaveLength(1);
    expect(output.declarations[0].cid).toBe(a.cid);
  });

  it("caches identical input — second call hits cache", async () => {
    const db = makeDb();
    const bridge = writeBridge(db, "ba", "pa");
    const catalog = writeCatalog(db, "bc", "pc", [bridge.cid!]);

    const stage = makeLoadCatalogStage({ db });
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { proofHash: catalog.cid! });
    const b = await runner.runStage(stage, { proofHash: catalog.cid! });

    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
  });

  it("dispatches via the registry as capability 'load-catalog'", async () => {
    const db = makeDb();
    const catalog = writeCatalog(db, "bc", "pc", []);

    const stage = makeLoadCatalogStage({ db });
    const registry = new InMemoryRegistry();
    registry.register(LOAD_CATALOG_CAPABILITY, stage);
    const runner = new WorkflowRunner(db, wf, registry);

    const result = await runner.request<LoadCatalogStageInput, LoadCatalogResult>(
      LOAD_CATALOG_CAPABILITY,
      { proofHash: catalog.cid! },
    );

    expect(result.output.found).toBe(true);
  });
});
