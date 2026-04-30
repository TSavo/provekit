/**
 * enumerate-local-leaves Stage tests. Drives a real in-memory SQLite via
 * makeDb + writeMemento — the canonical fixture pattern in this repo
 * (loadCatalog.test.ts, explain.test.ts). Pinning a small fixture set
 * keeps assertions on the projection alone; no mocks needed.
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
  makeEnumerateLocalLeavesStage,
  ENUMERATE_LOCAL_LEAVES_CAPABILITY,
} from "./enumerateLocalLeaves.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "enumerate-leaves-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("enumerate-local-leaves Stage", () => {
  it("declares its capability constant", () => {
    expect(ENUMERATE_LOCAL_LEAVES_CAPABILITY).toBe("enumerate-local-leaves");
  });

  it("returns an empty list when the local store is empty", async () => {
    const db = makeDb();
    const stage = makeEnumerateLocalLeavesStage({ db });
    const result = await stage.run({});
    expect(result.kindFilter).toBeNull();
    expect(result.producedByFilter).toBeNull();
    expect(result.leaves).toEqual([]);
  });

  it("lists every locally-minted memento, sorted by CID", async () => {
    const db = makeDb();
    const a = writeMemento(db, {
      bindingHash: "bh-a",
      propertyHash: "ph-a",
      verdict: "holds",
      witness: "w-a",
      producedBy: "ts-kit@1.0",
    });
    const b = writeMemento(db, {
      bindingHash: "bh-b",
      propertyHash: "ph-b",
      verdict: "violated",
      witness: "w-b",
      producedBy: "z3@4.12",
    });

    const stage = makeEnumerateLocalLeavesStage({ db });
    const result = await stage.run({});

    expect(result.leaves).toHaveLength(2);
    const cids = result.leaves.map((l) => l.cid);
    expect(cids).toEqual([...cids].sort());
    expect(new Set(cids)).toEqual(new Set([a.cid!, b.cid!]));
    const aLeaf = result.leaves.find((l) => l.cid === a.cid)!;
    expect(aLeaf.bindingHash).toBe("bh-a");
    expect(aLeaf.propertyHash).toBe("ph-a");
    expect(aLeaf.verdict).toBe("holds");
    expect(aLeaf.producedBy).toBe("ts-kit@1.0");
    expect(aLeaf.evidenceKind).toBe("legacy-witness");
  });

  it("filters by producedBy", async () => {
    const db = makeDb();
    writeMemento(db, {
      bindingHash: "bh-1",
      propertyHash: "ph-1",
      verdict: "holds",
      witness: "x",
      producedBy: "ts-kit@1.0",
    });
    writeMemento(db, {
      bindingHash: "bh-2",
      propertyHash: "ph-2",
      verdict: "holds",
      witness: "y",
      producedBy: "z3@4.12",
    });

    const stage = makeEnumerateLocalLeavesStage({ db });
    const result = await stage.run({ producedByFilter: "ts-kit@1.0" });
    expect(result.producedByFilter).toBe("ts-kit@1.0");
    expect(result.leaves).toHaveLength(1);
    expect(result.leaves[0].producedBy).toBe("ts-kit@1.0");
  });

  it("filters by evidence-variant kind", async () => {
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

    const stage = makeEnumerateLocalLeavesStage({ db });
    const result = await stage.run({ kindFilter: "lint-pass" });
    expect(result.kindFilter).toBe("lint-pass");
    expect(result.leaves).toHaveLength(1);
    expect(result.leaves[0].evidenceKind).toBe("lint-pass");
    expect(result.leaves[0].producedBy).toBe("p2");
  });

  it("preserves inputCids in the projection, sorted", async () => {
    const db = makeDb();
    const leaf = writeMemento(db, {
      bindingHash: "bh-leaf",
      propertyHash: "ph-leaf",
      verdict: "holds",
      witness: "leaf",
      producedBy: "p",
    });
    const second = writeMemento(db, {
      bindingHash: "bh-second",
      propertyHash: "ph-second",
      verdict: "holds",
      witness: "second",
      producedBy: "p",
    });
    writeMemento(db, {
      bindingHash: "bh-root",
      propertyHash: "ph-root",
      verdict: "holds",
      witness: "root",
      producedBy: "p",
      inputCids: [second.cid!, leaf.cid!],
    });

    const stage = makeEnumerateLocalLeavesStage({ db });
    const result = await stage.run({});
    const root = result.leaves.find((l) => l.bindingHash === "bh-root")!;
    expect(root.inputCids).toEqual([leaf.cid!, second.cid!].sort());
  });

  it("round-trips through serializeOutput / deserializeOutput", async () => {
    const db = makeDb();
    writeMemento(db, {
      bindingHash: "bh",
      propertyHash: "ph",
      verdict: "holds",
      witness: "w",
      producedBy: "p",
    });
    const stage = makeEnumerateLocalLeavesStage({ db });
    const result = await stage.run({});
    const witness = stage.serializeOutput(result);
    expect(stage.deserializeOutput(witness)).toEqual(result);
  });
});
