/**
 * enumerate-local-roots Stage tests. The Stage's contract is the
 * set difference: union of every local memento's inputCids minus the set
 * of local CIDs. Tests pin small fixtures and assert the projection.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { writeMemento } from "../../fix/runtime/mementoStore.js";
import {
  makeEnumerateLocalRootsStage,
  ENUMERATE_LOCAL_ROOTS_CAPABILITY,
} from "./enumerateLocalRoots.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "enumerate-roots-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("enumerate-local-roots Stage", () => {
  it("declares its capability constant", () => {
    expect(ENUMERATE_LOCAL_ROOTS_CAPABILITY).toBe("enumerate-local-roots");
  });

  it("returns no roots when the store is empty", async () => {
    const db = makeDb();
    const stage = makeEnumerateLocalRootsStage({ db });
    const result = await stage.run({});
    expect(result.roots).toEqual([]);
  });

  it("returns no roots when every inputCid is locally minted", async () => {
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
      inputCids: [leaf.cid!],
    });

    const stage = makeEnumerateLocalRootsStage({ db });
    const result = await stage.run({});
    expect(result.roots).toEqual([]);
  });

  it("surfaces external CIDs referenced via inputCids that aren't locally minted", async () => {
    const db = makeDb();
    writeMemento(db, {
      bindingHash: "bh-1",
      propertyHash: "ph-1",
      verdict: "holds",
      witness: "w1",
      producedBy: "p",
      inputCids: ["external-zeta", "external-alpha"],
    });
    writeMemento(db, {
      bindingHash: "bh-2",
      propertyHash: "ph-2",
      verdict: "holds",
      witness: "w2",
      producedBy: "p",
      inputCids: ["external-alpha", "external-mid"],
    });

    const stage = makeEnumerateLocalRootsStage({ db });
    const result = await stage.run({});
    expect(result.roots).toEqual([
      "external-alpha",
      "external-mid",
      "external-zeta",
    ]);
  });

  it("excludes locally-minted CIDs even when other mementos reference them", async () => {
    const db = makeDb();
    const leaf = writeMemento(db, {
      bindingHash: "bh-leaf",
      propertyHash: "ph-leaf",
      verdict: "holds",
      witness: "leaf",
      producedBy: "p",
    });
    writeMemento(db, {
      bindingHash: "bh-mixed",
      propertyHash: "ph-mixed",
      verdict: "holds",
      witness: "mixed",
      producedBy: "p",
      inputCids: [leaf.cid!, "truly-external"],
    });

    const stage = makeEnumerateLocalRootsStage({ db });
    const result = await stage.run({});
    expect(result.roots).toEqual(["truly-external"]);
  });

  it("round-trips through serializeOutput / deserializeOutput", async () => {
    const db = makeDb();
    writeMemento(db, {
      bindingHash: "bh",
      propertyHash: "ph",
      verdict: "holds",
      witness: "w",
      producedBy: "p",
      inputCids: ["external-cid"],
    });
    const stage = makeEnumerateLocalRootsStage({ db });
    const result = await stage.run({});
    const witness = stage.serializeOutput(result);
    expect(stage.deserializeOutput(witness)).toEqual(result);
  });
});
