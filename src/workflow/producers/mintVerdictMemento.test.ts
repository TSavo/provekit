/**
 * mint-verdict-memento Action tests. Asserts the verdict memento is
 * written at the ORIGINAL (bindingHash, propertyHash) the locate-memento
 * Stage recovered, with the right verdict and evidence variant.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb, type Db } from "../../db/index.js";
import { findMemento } from "../../fix/runtime/mementoStore.js";
import {
  makeMintVerdictMementoAction,
  MINT_VERDICT_MEMENTO_CAPABILITY,
} from "./mintVerdictMemento.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb(): Db {
  const tmp = mkdtempSync(join(tmpdir(), "mint-verdict-test-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("mint-verdict-memento action", () => {
  let db: Db;

  beforeEach(() => {
    db = makeDb();
  });

  it("has the expected capability constant", () => {
    expect(MINT_VERDICT_MEMENTO_CAPABILITY).toBe("mint-verdict-memento");
  });

  it("maps z3 unsat to property verdict 'holds' with z3-unsat evidence", async () => {
    const action = makeMintVerdictMementoAction({ db });
    const resource = await action.run({
      bindingHash: "bind1111111111aa",
      propertyHash: "prop1111111111aa",
      z3Verdict: "unsat",
      smtLib: "(check-sat)\n",
      z3RunMs: 12,
      inputCids: ["cidA", "cidB"],
      producedBy: "z3-symbolic@4.13.4",
    });
    expect(resource.verdict).toBe("holds");

    const memento = findMemento(db, {
      bindingHash: "bind1111111111aa",
      propertyHash: "prop1111111111aa",
    });
    expect(memento).not.toBeNull();
    expect(memento!.verdict).toBe("holds");
    expect(memento!.producedBy).toBe("z3-symbolic@4.13.4");
    expect(memento!.evidence?.kind).toBe("z3-unsat");
    expect(memento!.inputCids).toEqual(["cidA", "cidB"]);
  });

  it("maps z3 sat to property verdict 'violated' with z3-model evidence", async () => {
    const action = makeMintVerdictMementoAction({ db });
    const resource = await action.run({
      bindingHash: "bind2222222222bb",
      propertyHash: "prop2222222222bb",
      z3Verdict: "sat",
      smtLib: "(check-sat)\n",
      z3RunMs: 5,
      counterexample: { x: { sort: "Int", bigintString: "0" } },
      inputCids: [],
      producedBy: "z3-symbolic@4.13.4",
    });
    expect(resource.verdict).toBe("violated");

    const memento = findMemento(db, {
      bindingHash: "bind2222222222bb",
      propertyHash: "prop2222222222bb",
    });
    expect(memento).not.toBeNull();
    expect(memento!.verdict).toBe("violated");
    expect(memento!.evidence?.kind).toBe("z3-model");
    if (memento!.evidence?.kind === "z3-model") {
      expect(memento!.evidence.body.z3Verdict).toBe("sat");
      expect(memento!.evidence.body.counterexample).toMatchObject({
        x: { sort: "Int", bigintString: "0" },
      });
    }
  });

  it("maps z3 timeout to property verdict 'undecidable' with legacy-witness evidence", async () => {
    const action = makeMintVerdictMementoAction({ db });
    const resource = await action.run({
      bindingHash: "bind3333333333cc",
      propertyHash: "prop3333333333cc",
      z3Verdict: "timeout",
      smtLib: "(check-sat)\n",
      z3RunMs: 30000,
      inputCids: [],
      producedBy: "z3-symbolic@4.13.4",
    });
    expect(resource.verdict).toBe("undecidable");

    const memento = findMemento(db, {
      bindingHash: "bind3333333333cc",
      propertyHash: "prop3333333333cc",
    });
    expect(memento).not.toBeNull();
    expect(memento!.verdict).toBe("undecidable");
    expect(memento!.evidence?.kind).toBe("legacy-witness");
  });

  it("maps z3 unknown to 'undecidable'", async () => {
    const action = makeMintVerdictMementoAction({ db });
    const resource = await action.run({
      bindingHash: "bind4444444444dd",
      propertyHash: "prop4444444444dd",
      z3Verdict: "unknown",
      smtLib: "(check-sat)\n",
      z3RunMs: 100,
      inputCids: [],
      producedBy: "z3-symbolic@4.13.4",
    });
    expect(resource.verdict).toBe("undecidable");
  });

  it("falls back to the Action's own producedBy when input.producedBy is omitted", async () => {
    const action = makeMintVerdictMementoAction({ db });
    await action.run({
      bindingHash: "bind5555555555ee",
      propertyHash: "prop5555555555ee",
      z3Verdict: "unsat",
      smtLib: "(check-sat)\n",
      z3RunMs: 1,
      inputCids: [],
    });
    const memento = findMemento(db, {
      bindingHash: "bind5555555555ee",
      propertyHash: "prop5555555555ee",
    });
    expect(memento).not.toBeNull();
    expect(memento!.producedBy).toBe("mint-verdict-memento@v1");
  });

  it("describeResource returns a human-readable summary", () => {
    const action = makeMintVerdictMementoAction({ db });
    expect(
      action.describeResource({ cid: "abc123", verdict: "violated" }),
    ).toBe("verdict violated memento abc123");
  });

  it("serializeInput stable-sorts inputCids for the audit memento hash", () => {
    const action = makeMintVerdictMementoAction({ db });
    const a = action.serializeInput({
      bindingHash: "b",
      propertyHash: "p",
      z3Verdict: "unsat",
      smtLib: "x",
      z3RunMs: 1,
      inputCids: ["b", "a", "c"],
      producedBy: "p@v1",
    });
    expect(a).toEqual({
      bindingHash: "b",
      propertyHash: "p",
      z3Verdict: "unsat",
      producedBy: "p@v1",
      inputCids: ["a", "b", "c"],
    });
  });

  it("excludes smtLib + z3RunMs from serializeInput (those are content, not identity)", () => {
    const action = makeMintVerdictMementoAction({ db });
    const a = action.serializeInput({
      bindingHash: "b",
      propertyHash: "p",
      z3Verdict: "unsat",
      smtLib: "X",
      z3RunMs: 1,
      inputCids: [],
      producedBy: "p@v1",
    }) as Record<string, unknown>;
    expect("smtLib" in a).toBe(false);
    expect("z3RunMs" in a).toBe(false);
  });
});
