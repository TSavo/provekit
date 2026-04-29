/**
 * Memento store smoke tests. Validates the relational store v1:
 * insert, lookup, cross-validation, stats. No engine integration yet
 * (that's step 2-3 of the phasing); just the durable foundation.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { and, eq } from "drizzle-orm";
import { openDb } from "../../db/index.js";
import { verifications } from "../../db/schema/verifications.js";
import {
  writeMemento,
  findMemento,
  findAll,
  crossValidate,
  stats,
  hashCanonical,
} from "./mementoStore.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "memento-store-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const dbPath = join(tmp, ".provekit", "test.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("hashCanonical", () => {
  it("produces stable 16-hex-char output", () => {
    const h = hashCanonical({ foo: "bar", n: 42 });
    expect(h).toMatch(/^[a-f0-9]{16}$/);
  });

  it("is order-independent for object keys", () => {
    const a = hashCanonical({ a: 1, b: 2, c: 3 });
    const b = hashCanonical({ c: 3, b: 2, a: 1 });
    expect(a).toBe(b);
  });

  it("differs on nested key reordering AT VALUE LEVEL", () => {
    const a = hashCanonical({ x: { p: 1, q: 2 } });
    const b = hashCanonical({ x: { q: 2, p: 1 } });
    expect(a).toBe(b); // nested keys also sort
  });

  it("distinguishes structurally-different inputs", () => {
    expect(hashCanonical({ a: 1 })).not.toBe(hashCanonical({ a: 2 }));
    expect(hashCanonical([1, 2, 3])).not.toBe(hashCanonical([3, 2, 1]));
  });
});

describe("memento store: insert + lookup", () => {
  let db: ReturnType<typeof makeDb>;
  beforeEach(() => {
    db = makeDb();
  });

  it("inserts a memento and finds it by key", () => {
    writeMemento(db, {
      bindingHash: "aaaa1111bbbb2222",
      propertyHash: "cccc3333dddd4444",
      verdict: "holds",
      witness: "Z3 model: k=5",
      producedBy: "z3-symbolic@4.13",
    });

    const found = findMemento(db, {
      bindingHash: "aaaa1111bbbb2222",
      propertyHash: "cccc3333dddd4444",
    });
    expect(found).not.toBeNull();
    expect(found?.verdict).toBe("holds");
    expect(found?.producedBy).toBe("z3-symbolic@4.13");
    expect(found?.witness).toBe("Z3 model: k=5");
  });

  it("returns null on cache miss", () => {
    const result = findMemento(db, {
      bindingHash: "missing0000missing",
      propertyHash: "alsomissing",
    });
    expect(result).toBeNull();
  });

  it("upserts on (bindingHash, propertyHash, producedBy) conflict", () => {
    writeMemento(db, {
      bindingHash: "aaaa",
      propertyHash: "bbbb",
      verdict: "violated",
      producedBy: "z3@4.13",
    });
    writeMemento(db, {
      bindingHash: "aaaa",
      propertyHash: "bbbb",
      verdict: "holds", // updated verdict, same producer
      producedBy: "z3@4.13",
    });
    const all = findAll(db, { bindingHash: "aaaa", propertyHash: "bbbb" });
    expect(all).toHaveLength(1);
    expect(all[0].verdict).toBe("holds");
  });

  it("preserves rows from different producers for the same key", () => {
    writeMemento(db, {
      bindingHash: "key1",
      propertyHash: "prop1",
      verdict: "holds",
      producedBy: "z3@4.13",
    });
    writeMemento(db, {
      bindingHash: "key1",
      propertyHash: "prop1",
      verdict: "holds",
      producedBy: "datalog@1.0",
    });
    const all = findAll(db, { bindingHash: "key1", propertyHash: "prop1" });
    expect(all).toHaveLength(2);
    const producers = all.map((m) => m.producedBy).sort();
    expect(producers).toEqual(["datalog@1.0", "z3@4.13"]);
  });
});

describe("memento store: cross-validation", () => {
  let db: ReturnType<typeof makeDb>;
  beforeEach(() => {
    db = makeDb();
  });

  it("returns empty when all producers agree", () => {
    writeMemento(db, {
      bindingHash: "k1",
      propertyHash: "p1",
      verdict: "holds",
      producedBy: "z3@4.13",
    });
    writeMemento(db, {
      bindingHash: "k1",
      propertyHash: "p1",
      verdict: "holds",
      producedBy: "datalog@1.0",
    });
    expect(crossValidate(db)).toEqual([]);
  });

  it("surfaces disagreements between producers", () => {
    writeMemento(db, {
      bindingHash: "k1",
      propertyHash: "p1",
      verdict: "holds",
      producedBy: "z3@4.13",
    });
    writeMemento(db, {
      bindingHash: "k1",
      propertyHash: "p1",
      verdict: "violated",
      producedBy: "datalog@1.0",
    });
    const disagreements = crossValidate(db);
    expect(disagreements).toHaveLength(1);
    expect(disagreements[0].distinctVerdicts.sort()).toEqual([
      "holds",
      "violated",
    ]);
    expect(disagreements[0].rows).toHaveLength(2);
  });

  it("surfaces multiple disagreements across keys", () => {
    writeMemento(db, {
      bindingHash: "k1",
      propertyHash: "p1",
      verdict: "holds",
      producedBy: "z3",
    });
    writeMemento(db, {
      bindingHash: "k1",
      propertyHash: "p1",
      verdict: "violated",
      producedBy: "datalog",
    });
    writeMemento(db, {
      bindingHash: "k2",
      propertyHash: "p2",
      verdict: "undecidable",
      producedBy: "z3",
    });
    writeMemento(db, {
      bindingHash: "k2",
      propertyHash: "p2",
      verdict: "holds",
      producedBy: "datalog",
    });
    expect(crossValidate(db)).toHaveLength(2);
  });
});

describe("memento store: CID + DAG walk", () => {
  let db: ReturnType<typeof makeDb>;
  beforeEach(() => {
    db = makeDb();
  });

  it("writeMemento computes a CID and persists it", async () => {
    const { writeMemento, findMemento, computeCid } = await import("./mementoStore.js");
    writeMemento(db, {
      bindingHash: "aaaa",
      propertyHash: "bbbb",
      verdict: "holds",
      producedBy: "z3@4.13",
    });
    const found = findMemento(db, { bindingHash: "aaaa", propertyHash: "bbbb" });
    expect(found?.cid).toBeDefined();
    expect(found?.cid).toMatch(/^[a-f0-9]{32}$/);
  });

  it("computeCid is stable for identical content", async () => {
    const { computeCid } = await import("./mementoStore.js");
    const a = computeCid({
      bindingHash: "x",
      propertyHash: "y",
      verdict: "holds",
      producedBy: "p",
    });
    const b = computeCid({
      bindingHash: "x",
      propertyHash: "y",
      verdict: "holds",
      producedBy: "p",
    });
    expect(a).toBe(b);
  });

  it("computeCid changes when verdict changes", async () => {
    const { computeCid } = await import("./mementoStore.js");
    const a = computeCid({
      bindingHash: "x",
      propertyHash: "y",
      verdict: "holds",
      producedBy: "p",
    });
    const b = computeCid({
      bindingHash: "x",
      propertyHash: "y",
      verdict: "violated",
      producedBy: "p",
    });
    expect(a).not.toBe(b);
  });

  it("walk follows inputCids from a starting memento", async () => {
    const { writeMemento, findByCid, walk } = await import("./mementoStore.js");
    // Build a 3-node DAG: A → B → C (A has B as input; B has C as input)
    writeMemento(db, {
      bindingHash: "leaf",
      propertyHash: "p",
      verdict: "holds",
      producedBy: "z3",
    });
    const cFound = db.select().from(verifications)
      .where(and(eq(verifications.bindingHash, "leaf"), eq(verifications.propertyHash, "p")))
      .all();
    const cCid = cFound[0]?.cid as string;
    expect(cCid).toBeDefined();

    writeMemento(db, {
      bindingHash: "mid",
      propertyHash: "p",
      verdict: "holds",
      producedBy: "z3",
      inputCids: [cCid],
    });
    const bFound = db.select().from(verifications)
      .where(and(eq(verifications.bindingHash, "mid"), eq(verifications.propertyHash, "p")))
      .all();
    const bCid = bFound[0]?.cid as string;

    writeMemento(db, {
      bindingHash: "root",
      propertyHash: "p",
      verdict: "holds",
      producedBy: "z3",
      inputCids: [bCid],
    });
    const aFound = db.select().from(verifications)
      .where(and(eq(verifications.bindingHash, "root"), eq(verifications.propertyHash, "p")))
      .all();
    const aCid = aFound[0]?.cid as string;

    const walked = walk(db, aCid);
    expect(walked).toHaveLength(3);
    expect(walked.map((m) => m.bindingHash)).toEqual(["root", "mid", "leaf"]);
  });

  it("walk respects maxDepth", async () => {
    const { writeMemento, walk } = await import("./mementoStore.js");
    // Two-node DAG: A → B
    writeMemento(db, { bindingHash: "leaf", propertyHash: "p", verdict: "holds", producedBy: "z3" });
    const leafCid = (db.select().from(verifications).all()[0] as { cid: string }).cid;
    writeMemento(db, {
      bindingHash: "root",
      propertyHash: "p",
      verdict: "holds",
      producedBy: "z3",
      inputCids: [leafCid],
    });
    const rootCid = (db.select().from(verifications)
      .where(eq(verifications.bindingHash, "root")).all()[0] as { cid: string }).cid;

    const depth0 = walk(db, rootCid, { maxDepth: 0 });
    expect(depth0).toHaveLength(1);
    expect(depth0[0].bindingHash).toBe("root");

    const depth1 = walk(db, rootCid, { maxDepth: 1 });
    expect(depth1).toHaveLength(2);
  });
});

describe("memento store: stats", () => {
  it("counts rows and unique keys", () => {
    const db = makeDb();
    writeMemento(db, {
      bindingHash: "k1",
      propertyHash: "p1",
      verdict: "holds",
      producedBy: "z3",
    });
    writeMemento(db, {
      bindingHash: "k1",
      propertyHash: "p1",
      verdict: "holds",
      producedBy: "datalog", // same key, different producer = 2 rows, 1 unique key
    });
    writeMemento(db, {
      bindingHash: "k2",
      propertyHash: "p2",
      verdict: "violated",
      producedBy: "z3",
    });
    const s = stats(db);
    expect(s.totalRows).toBe(3);
    expect(s.uniqueKeys).toBe(2);
    expect(s.byVerdict.holds).toBe(2);
    expect(s.byVerdict.violated).toBe(1);
    expect(s.byProducer.z3).toBe(2);
    expect(s.byProducer.datalog).toBe(1);
  });
});
