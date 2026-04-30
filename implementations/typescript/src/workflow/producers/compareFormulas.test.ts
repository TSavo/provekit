/**
 * CompareFormulas stage tests. Cache, capability dispatch, and the
 * structural delta semantics under weaken vs strengthen mode.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { WorkflowRunner } from "../runner.js";
import {
  makeCompareFormulasStage,
  COMPARE_FORMULAS_CAPABILITY,
} from "./compareFormulas.js";
import type { IrFormula, IrTerm, Sort } from "../../ir/formulas.js";
import { propertyHashFromFormula } from "../../canonicalizer/canonicalize.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "compare-formulas-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-compare-formulas-test-v1" };

const intSort: Sort = { kind: "primitive", name: "Int" };
const var_x: IrTerm = { kind: "var", name: "x", sort: intSort };
const const_0: IrTerm = { kind: "const", value: 0, sort: intSort };
const const_5: IrTerm = { kind: "const", value: 5, sort: intSort };

function atomic(predicate: string, args: IrTerm[]): IrFormula {
  return { kind: "atomic", predicate, args };
}

// Wrap each predicate in a forall so the canonicalizer's de Bruijn pass
// has bindings to resolve. The structural diff cares about the inner
// shape; the wrapper is just there to make the formula closeable.
function forallX(body: IrFormula): IrFormula {
  return {
    kind: "forall",
    sort: intSort,
    predicate: { kind: "lambda", varName: "x", sort: intSort, body },
  };
}

const xGt0: IrFormula = forallX(atomic(">", [var_x, const_0]));
const xLt5: IrFormula = forallX(atomic("<", [var_x, const_5]));
const xLte5: IrFormula = forallX(atomic("≤", [var_x, const_5]));

describe("compareFormulas Stage", () => {
  it("returns changed=false and empty deltas when formulas are identical", async () => {
    const db = makeDb();
    const stage = makeCompareFormulasStage();
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      oldFormula: xGt0,
      newFormula: xGt0,
      mode: "weaken",
    });

    expect(result.output.changed).toBe(false);
    expect(result.output.oldPropertyHash).toBe(result.output.newPropertyHash);
    expect(result.output.addedSubFormulaHashes).toEqual([]);
    expect(result.output.removedSubFormulaHashes).toEqual([]);
    expect(result.output.mode).toBe("weaken");
  });

  it("detects added conjuncts when strengthening", async () => {
    const db = makeDb();
    const stage = makeCompareFormulasStage();
    const runner = new WorkflowRunner(db, wf);

    const oldFormula: IrFormula = { kind: "and", conjuncts: [xGt0] };
    const newFormula: IrFormula = { kind: "and", conjuncts: [xGt0, xLt5] };

    const result = await runner.runStage(stage, {
      oldFormula,
      newFormula,
      mode: "strengthen",
    });

    expect(result.output.changed).toBe(true);
    expect(result.output.addedSubFormulaHashes).toEqual([
      propertyHashFromFormula(xLt5),
    ]);
    expect(result.output.removedSubFormulaHashes).toEqual([]);
    expect(result.output.mode).toBe("strengthen");
  });

  it("detects removed conjuncts when weakening", async () => {
    const db = makeDb();
    const stage = makeCompareFormulasStage();
    const runner = new WorkflowRunner(db, wf);

    const oldFormula: IrFormula = { kind: "and", conjuncts: [xGt0, xLt5] };
    const newFormula: IrFormula = { kind: "and", conjuncts: [xGt0] };

    const result = await runner.runStage(stage, {
      oldFormula,
      newFormula,
      mode: "weaken",
    });

    expect(result.output.changed).toBe(true);
    expect(result.output.addedSubFormulaHashes).toEqual([]);
    expect(result.output.removedSubFormulaHashes).toEqual([
      propertyHashFromFormula(xLt5),
    ]);
  });

  it("reports both adds and removes when conjuncts swap", async () => {
    const db = makeDb();
    const stage = makeCompareFormulasStage();
    const runner = new WorkflowRunner(db, wf);

    // x < 5 → x ≤ 5 — looser bound, expressed as a swap
    const oldFormula: IrFormula = { kind: "and", conjuncts: [xGt0, xLt5] };
    const newFormula: IrFormula = { kind: "and", conjuncts: [xGt0, xLte5] };

    const result = await runner.runStage(stage, {
      oldFormula,
      newFormula,
      mode: "weaken",
    });

    expect(result.output.changed).toBe(true);
    expect(result.output.addedSubFormulaHashes).toEqual([
      propertyHashFromFormula(xLte5),
    ]);
    expect(result.output.removedSubFormulaHashes).toEqual([
      propertyHashFromFormula(xLt5),
    ]);
  });

  it("caches identical input — second run is a hit", async () => {
    const db = makeDb();
    const stage = makeCompareFormulasStage();
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      oldFormula: xGt0,
      newFormula: xLt5,
      mode: "weaken",
    });
    const b = await runner.runStage(stage, {
      oldFormula: xGt0,
      newFormula: xLt5,
      mode: "weaken",
    });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
  });

  it("capability constant matches the conventional name", () => {
    expect(COMPARE_FORMULAS_CAPABILITY).toBe("compare-formulas");
  });
});
