/**
 * locate-memento Stage tests. Mints a real formulate-via-lifter-shaped
 * memento via writeMemento, then asserts the locate-memento Stage
 * recovers the IrFormula round-trip.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb, type Db } from "../../db/index.js";
import { writeMemento } from "../../fix/runtime/mementoStore.js";
import type { IrFormula } from "../../ir/formulas.js";
import {
  makeLocateMementoStage,
  LOCATE_MEMENTO_CAPABILITY,
  LocateMementoError,
} from "./locateMemento.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb(): Db {
  const tmp = mkdtempSync(join(tmpdir(), "locate-memento-test-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const sampleFormula: IrFormula = {
  kind: "forall",
  sort: { kind: "primitive", name: "Int" },
  predicate: {
    kind: "lambda",
    varName: "_x0",
    sort: { kind: "primitive", name: "Int" },
    body: {
      kind: "atomic",
      predicate: ">",
      args: [
        { kind: "var", name: "_x0", sort: { kind: "primitive", name: "Int" } },
        { kind: "const", value: 0, sort: { kind: "primitive", name: "Int" } },
      ],
    },
  },
};

function mintFormulateMemento(
  db: Db,
  args: {
    propertyHash: string;
    bindingHash: string;
    formula: IrFormula;
    producedBy?: string;
  },
): string {
  const witnessPayload = {
    surfaceText: "property('positive', forAll<Int>(x => x > 0));",
    formula: args.formula,
    propertyHash: args.propertyHash,
    name: "positive",
    inputCidsToCompose: [],
  };
  const row = writeMemento(db, {
    bindingHash: args.bindingHash,
    propertyHash: args.propertyHash,
    verdict: "holds",
    witness: JSON.stringify(witnessPayload),
    producedBy: args.producedBy ?? "formulate-via-lifter@v1",
  });
  return row.cid!;
}

describe("locate-memento stage", () => {
  let db: Db;

  beforeEach(() => {
    db = makeDb();
  });

  it("has the expected capability constant", () => {
    expect(LOCATE_MEMENTO_CAPABILITY).toBe("locate-memento");
  });

  it("recovers the IrFormula round-trip for a freshly-minted memento", async () => {
    const cid = mintFormulateMemento(db, {
      propertyHash: "propA0000000000A",
      bindingHash: "bindA0000000000A",
      formula: sampleFormula,
    });

    const stage = makeLocateMementoStage({ db });
    const out = await stage.run({ propertyHash: "propA0000000000A" });

    expect(out.formula).toEqual(sampleFormula);
    expect(out.propertyHash).toBe("propA0000000000A");
    expect(out.bindingHash).toBe("bindA0000000000A");
    expect(out.sourceCid).toBe(cid);
    expect(out.sourceProducedBy).toBe("formulate-via-lifter@v1");
  });

  it("throws when no memento has the requested propertyHash", async () => {
    const stage = makeLocateMementoStage({ db });
    await expect(stage.run({ propertyHash: "missing0000000ff" })).rejects.toBeInstanceOf(
      LocateMementoError,
    );
  });

  it("prefers formulate-via-lifter producers over other producers", async () => {
    // Two mementos with the same propertyHash, different producers.
    // Insert the non-formulate one first so order alone wouldn't pick it.
    mintFormulateMemento(db, {
      propertyHash: "propB0000000000B",
      bindingHash: "bindOther000000B",
      formula: sampleFormula,
      producedBy: "other-producer@v1",
    });
    const fvlCid = mintFormulateMemento(db, {
      propertyHash: "propB0000000000B",
      bindingHash: "bindFvl00000000B",
      formula: sampleFormula,
      producedBy: "formulate-via-lifter@v1",
    });

    const stage = makeLocateMementoStage({ db });
    const out = await stage.run({ propertyHash: "propB0000000000B" });

    expect(out.sourceCid).toBe(fvlCid);
    expect(out.sourceProducedBy).toBe("formulate-via-lifter@v1");
  });

  it("falls back to a non-formulate producer if its witness round-trips a formula", async () => {
    const cid = mintFormulateMemento(db, {
      propertyHash: "propC0000000000C",
      bindingHash: "bindC0000000000C",
      formula: sampleFormula,
      producedBy: "swarm-imported@v2",
    });
    const stage = makeLocateMementoStage({ db });
    const out = await stage.run({ propertyHash: "propC0000000000C" });
    expect(out.sourceCid).toBe(cid);
    expect(out.formula).toEqual(sampleFormula);
  });

  it("throws when matching memento(s) carry no extractable formula", async () => {
    writeMemento(db, {
      bindingHash: "bindD0000000000D",
      propertyHash: "propD0000000000D",
      verdict: "holds",
      witness: JSON.stringify({ unrelated: "shape" }),
      producedBy: "other@v1",
    });
    const stage = makeLocateMementoStage({ db });
    await expect(stage.run({ propertyHash: "propD0000000000D" })).rejects.toBeInstanceOf(
      LocateMementoError,
    );
  });

  it("serializeInput depends only on propertyHash", () => {
    const stage = makeLocateMementoStage({ db });
    expect(stage.serializeInput({ propertyHash: "p1" })).toEqual({
      propertyHash: "p1",
    });
  });

  it("output round-trips through serialize/deserialize", () => {
    const stage = makeLocateMementoStage({ db });
    const sample = {
      formula: sampleFormula,
      propertyHash: "p1",
      bindingHash: "b1",
      sourceCid: "c1",
      sourceProducedBy: "producer@v1",
    };
    const witness = stage.serializeOutput(sample);
    expect(stage.deserializeOutput(witness)).toEqual(sample);
  });
});
