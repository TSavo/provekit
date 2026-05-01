/**
 * Parser/emitter conformance tests for the IR-JSON external grammar
 * (protocol/specs/2026-04-30-ir-formal-grammar.md).
 *
 * The previous test set covered cross-language fixtures shipped from
 * scripts/cross-lang-equivalence/; under v1.1 those fixtures were
 * regenerated outside this run. The tests below cover the in-process
 * round-trip property and the locked-key-order strict mode.
 */

import { describe, it, expect, beforeEach } from "vitest";
import {
  parseDocument,
  parseFormula,
  parseTerm,
  parseSort,
  emitDocument,
  GrammarParseError,
} from "./parse.js";
import type { Declaration } from "../symbolic/property.js";
import {
  _resetCollector,
  beginCollecting,
  contract,
  forAll,
} from "../symbolic/property.js";
import { Int } from "../sorts.js";
import { gt, num } from "../symbolic/primitives.js";
import { propertyHashFromFormula } from "../../canonicalizer/canonicalize.js";

describe("grammar parser — in-memory IR round-trip", () => {
  beforeEach(() => {
    _resetCollector();
  });

  it("round-trips a contract declaration assembled in-process", () => {
    const finish = beginCollecting();
    contract("simple", { pre: forAll(Int, (x) => gt(x, num(0))) });
    const decls = finish();

    const text = emitDocument(decls);
    const parsed = parseDocument(text, { strict: true });
    expect(parsed).toEqual(decls);
    const text2 = emitDocument(parsed);
    expect(text2).toBe(text);
  });
});

describe("grammar parser — declaration shape", () => {
  it("parses a contract declaration with pre+post", () => {
    const json = JSON.stringify([
      {
        kind: "contract",
        name: "p",
        outBinding: "out",
        pre: { kind: "atomic", name: "true", args: [] },
        post: { kind: "atomic", name: "false", args: [] },
      },
    ]);
    const decls = parseDocument(json);
    expect(decls).toHaveLength(1);
    const decl = decls[0]!;
    expect(decl.kind).toBe("contract");
    if (decl.kind === "contract") {
      expect(decl.name).toBe("p");
      expect(decl.outBinding).toBe("out");
      expect(decl.pre).toBeDefined();
      expect(decl.post).toBeDefined();
    }
  });

  it("rejects a contract declaration with no pre/post/inv", () => {
    const json = JSON.stringify([
      { kind: "contract", name: "empty", outBinding: "out" },
    ]);
    expect(() => parseDocument(json)).toThrowError(GrammarParseError);
  });

  it("rejects unknown declaration kinds", () => {
    expect(() => parseDocument(`[{"kind":"vapor"}]`)).toThrowError(/contract.*bridge/);
  });

  it("rejects the legacy property kind", () => {
    expect(() =>
      parseDocument(`[{"kind":"property","name":"x","formula":{"kind":"atomic","name":"true","args":[]}}]`),
    ).toThrowError(/contract.*bridge/);
  });

  it("parses a bridge declaration", () => {
    const json = JSON.stringify([
      {
        kind: "bridge",
        name: "b",
        sourceSymbol: "parseInt",
        sourceLayer: "ts",
        targetContractCid: "1234567890abcdef1234567890abcdef",
        targetLayer: "v8",
      },
    ]);
    const decls = parseDocument(json);
    expect(decls).toHaveLength(1);
    expect(decls[0]!.kind).toBe("bridge");
  });
});

describe("grammar parser — formula round-trip", () => {
  it("parses a flat-shape forall", () => {
    const json = JSON.stringify({
      kind: "forall",
      name: "_x0",
      sort: { kind: "primitive", name: "Int" },
      body: {
        kind: "atomic",
        name: ">",
        args: [
          { kind: "var", name: "_x0" },
          { kind: "const", value: 0, sort: { kind: "primitive", name: "Int" } },
        ],
      },
    });
    const f = parseFormula(json);
    expect(f.kind).toBe("forall");
    if (f.kind === "forall") {
      expect(f.name).toBe("_x0");
      expect(f.body.kind).toBe("atomic");
    }
  });

  it("parses connective with operands", () => {
    const json = JSON.stringify({
      kind: "and",
      operands: [
        { kind: "atomic", name: "true", args: [] },
        { kind: "atomic", name: "false", args: [] },
      ],
    });
    const f = parseFormula(json);
    expect(f.kind).toBe("and");
    if (f.kind === "and") {
      expect(f.operands).toHaveLength(2);
    }
  });

  it("rejects atomic with the legacy `predicate` field", () => {
    expect(() =>
      parseFormula(`{"kind":"atomic","predicate":"true","args":[]}`, { strict: true }),
    ).toThrowError(GrammarParseError);
  });

  it("rejects var term with extra `sort` field", () => {
    expect(() =>
      parseTerm(`{"kind":"var","name":"x","sort":{"kind":"primitive","name":"Int"}}`),
    ).toThrowError(GrammarParseError);
  });

  it("parses a primitive sort", () => {
    const s = parseSort(`{"kind":"primitive","name":"Int"}`);
    expect(s.kind).toBe("primitive");
  });
});

describe("grammar parser — propertyHash seam", () => {
  beforeEach(() => {
    _resetCollector();
  });

  it("propertyHashFromFormula(parsed) === propertyHashFromFormula(in-process)", () => {
    const finish = beginCollecting();
    contract("seam-test", { pre: forAll(Int, (x) => gt(x, num(0))) });
    const inDecls = finish();
    const inFirst = inDecls[0];
    if (!inFirst || inFirst.kind !== "contract" || !inFirst.pre) {
      throw new Error("expected contract decl with pre");
    }

    const text = emitDocument(inDecls);
    const parsed = parseDocument(text);
    const parsedFirst = parsed[0];
    if (!parsedFirst || parsedFirst.kind !== "contract" || !parsedFirst.pre) {
      throw new Error("expected contract decl with pre");
    }

    expect(propertyHashFromFormula(parsedFirst.pre)).toBe(
      propertyHashFromFormula(inFirst.pre),
    );
  });
});
