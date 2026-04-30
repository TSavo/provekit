/**
 * Tests for the IR grammar reference parser + emitter.
 *
 * Coverage:
 * - The three locked cross-language fixtures parse + round-trip.
 * - Every node kind has a positive parse fixture (hand-built).
 * - Negative fixtures: malformed inputs are rejected with structured errors.
 * - Strict mode enforces locked key order; non-strict mode accepts reorders.
 * - The emitter produces canonical key order regardless of parse order.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { execSync } from "node:child_process";
import * as path from "node:path";

import {
  emitDocument,
  GrammarParseError,
  parseDocument,
  parseFormula,
  parseSort,
  parseTerm,
} from "./parse.js";

import type { Declaration } from "../symbolic/property.js";
import {
  _resetCollector,
  beginCollecting,
  property,
  forAll,
  exists,
  and,
  or,
  not,
  implies,
} from "../symbolic/property.js";
import { Int, Bool, BV } from "../sorts.js";
import { eq, gt } from "../symbolic/primitives.js";
import { num } from "../symbolic/primitives.js";
import { propertyHashFromFormula } from "../../canonicalizer/canonicalize.js";

// ---------------------------------------------------------------------------
// 1. Cross-language fixture round-trip
// ---------------------------------------------------------------------------

describe("grammar parser — cross-language fixtures", () => {
  beforeEach(() => {
    _resetCollector();
  });

  const FIXTURES = [
    "forall_int_gt_zero",
    "eq_parseint_zero_zero",
    "forall_string_parseint_gte_zero",
  ] as const;

  // The TS kit's runner emits each fixture's canonical JSON. We re-create
  // the exact code the runner uses and compare.
  function runTsRunner(fixture: string): string {
    const runnerPath = path.resolve(
      __dirname,
      "../../../scripts/cross-lang-equivalence/ts-runner.ts",
    );
    return execSync(`npx tsx "${runnerPath}" ${fixture}`, {
      encoding: "utf8",
    }).toString();
  }

  for (const fixture of FIXTURES) {
    it(`parses + round-trips fixture "${fixture}"`, () => {
      const json = runTsRunner(fixture);
      // Parse strict — fixtures must conform to locked key order.
      const decls = parseDocument(json, { strict: true });
      expect(decls).toHaveLength(1);
      // Re-emit and check byte-equality with the kit's output.
      const reEmitted = emitDocument(decls);
      expect(reEmitted).toBe(json);
      // Re-parse the re-emit and check structural equality with the first parse.
      const decls2 = parseDocument(reEmitted, { strict: true });
      expect(decls2).toEqual(decls);
    });
  }
});

// ---------------------------------------------------------------------------
// 2. Round-trip from in-memory IR (no kit involvement)
// ---------------------------------------------------------------------------

describe("grammar parser — in-memory IR round-trip", () => {
  beforeEach(() => {
    _resetCollector();
  });

  it("round-trips a property declaration assembled in-process", () => {
    const finish = beginCollecting();
    property("simple", forAll(Int, (x) => gt(x, num(0))));
    const decls = finish();

    const text = emitDocument(decls);
    const parsed = parseDocument(text, { strict: true });
    expect(parsed).toEqual(decls);
    // The emitter's output is itself a fixed point.
    const text2 = emitDocument(parsed);
    expect(text2).toBe(text);
  });

  it("round-trips a bridge declaration with notes present", () => {
    const decls: Declaration[] = [
      {
        kind: "bridge",
        name: "string-bridge",
        sourceSymbol: "parseInt",
        sourceLayer: "ts",
        targetContractCid: "bafy...",
        targetLayer: "core",
        notes: "see RFC",
      },
    ];
    const text = emitDocument(decls);
    expect(text).toContain('"notes":"see RFC"');
    expect(parseDocument(text, { strict: true })).toEqual(decls);
  });

  it("round-trips a bridge declaration with notes absent (notes key omitted entirely)", () => {
    const decls: Declaration[] = [
      {
        kind: "bridge",
        name: "no-notes-bridge",
        sourceSymbol: "f",
        sourceLayer: "ts",
        targetContractCid: "bafy...",
        targetLayer: "core",
      },
    ];
    const text = emitDocument(decls);
    expect(text).not.toContain('"notes"');
    expect(parseDocument(text, { strict: true })).toEqual(decls);
  });

  it("round-trips and / or / not / implies", () => {
    const finish = beginCollecting();
    property(
      "complex",
      and(
        or(
          not(forAll(Int, (x) => gt(x, num(0)))),
          implies(eq(num(0), num(0)), exists(Bool, (_b) => eq(num(1), num(1)))),
        ),
      ),
    );
    const decls = finish();
    const text = emitDocument(decls);
    const parsed = parseDocument(text, { strict: true });
    expect(parsed).toEqual(decls);
  });

  it("round-trips a bitvec sort and atomic predicate", () => {
    const sort = BV(64);
    const text = emitDocument([
      {
        kind: "property",
        name: "bv-prop",
        formula: {
          kind: "atomic",
          predicate: "bvugt",
          args: [
            { kind: "var", name: "x", sort },
            { kind: "const", value: 0, sort },
          ],
        },
      },
    ]);
    const parsed = parseDocument(text, { strict: true });
    expect(parsed[0]?.kind).toBe("property");
    expect(text).toContain('"kind":"bitvec"');
    expect(text).toContain('"width":64');
  });

  it("round-trips set / tuple / function sorts", () => {
    const decls: Declaration[] = [
      {
        kind: "property",
        name: "shape-only",
        formula: {
          kind: "atomic",
          predicate: "true",
          args: [
            {
              kind: "var",
              name: "f",
              sort: {
                kind: "function",
                domain: [
                  { kind: "set", element: { kind: "primitive", name: "Int" } },
                  {
                    kind: "tuple",
                    elements: [
                      { kind: "primitive", name: "Bool" },
                      { kind: "bitvec", width: 8 },
                    ],
                  },
                ],
                range: { kind: "primitive", name: "Bool" },
              },
            },
          ],
        },
      },
    ];
    const text = emitDocument(decls);
    expect(parseDocument(text, { strict: true })).toEqual(decls);
  });

  it("round-trips const values of every accepted shape (number, string, bool, null)", () => {
    const decls: Declaration[] = [
      {
        kind: "property",
        name: "const-shapes",
        formula: {
          kind: "atomic",
          predicate: "true",
          args: [
            { kind: "const", value: 42, sort: { kind: "primitive", name: "Int" } },
            { kind: "const", value: "hi", sort: { kind: "primitive", name: "String" } },
            { kind: "const", value: true, sort: { kind: "primitive", name: "Bool" } },
            { kind: "const", value: null, sort: { kind: "primitive", name: "Ref" } },
          ],
        },
      },
    ];
    const text = emitDocument(decls);
    expect(parseDocument(text, { strict: true })).toEqual(decls);
  });

  it("round-trips a ctor term with zero args (nullary constructor)", () => {
    const decls: Declaration[] = [
      {
        kind: "property",
        name: "nullary-ctor",
        formula: {
          kind: "atomic",
          predicate: "=",
          args: [
            {
              kind: "ctor",
              name: "currentTime",
              args: [],
              sort: { kind: "primitive", name: "Int" },
            },
            { kind: "const", value: 0, sort: { kind: "primitive", name: "Int" } },
          ],
        },
      },
    ];
    const text = emitDocument(decls);
    expect(text).toContain('"args":[]');
    expect(parseDocument(text, { strict: true })).toEqual(decls);
  });

  it("accepts an empty document", () => {
    expect(parseDocument("[]", { strict: true })).toEqual([]);
    expect(emitDocument([])).toBe("[]");
  });
});

// ---------------------------------------------------------------------------
// 3. Negative parse cases — every error path
// ---------------------------------------------------------------------------

describe("grammar parser — malformed input rejection", () => {
  it("rejects malformed JSON", () => {
    expect(() => parseDocument("{not json")).toThrowError(GrammarParseError);
  });

  it("rejects a non-array document", () => {
    expect(() => parseDocument(`{"kind":"property"}`)).toThrowError(/JSON array/);
  });

  it("rejects an unknown declaration kind", () => {
    expect(() => parseDocument(`[{"kind":"vapor"}]`)).toThrowError(/property.*bridge/);
  });

  it("rejects extra keys on a property declaration", () => {
    const json = `[{"kind":"property","name":"x","formula":{"kind":"atomic","predicate":"true","args":[]},"extra":1}]`;
    expect(() => parseDocument(json)).toThrowError(/unexpected key/);
  });

  it("rejects missing required field on a property declaration", () => {
    expect(() => parseDocument(`[{"kind":"property","name":"x"}]`)).toThrowError(
      /required key "formula"/,
    );
  });

  it("rejects unknown formula kind", () => {
    expect(() =>
      parseDocument(
        `[{"kind":"property","name":"p","formula":{"kind":"emote","body":"!"}}]`,
      ),
    ).toThrowError(/forall.*atomic/);
  });

  it("rejects unknown sort kind", () => {
    expect(() =>
      parseSort(`{"kind":"hyperbitvec","width":42}`),
    ).toThrowError(/primitive.*bitvec.*set.*tuple.*function/);
  });

  it("rejects bitvec with non-positive width", () => {
    expect(() => parseSort(`{"kind":"bitvec","width":0}`)).toThrowError(/positive integer/);
    expect(() => parseSort(`{"kind":"bitvec","width":-1}`)).toThrowError(/positive integer/);
    expect(() => parseSort(`{"kind":"bitvec","width":1.5}`)).toThrowError(/positive integer/);
  });

  it("rejects const with object value", () => {
    expect(() =>
      parseTerm(`{"kind":"const","value":{},"sort":{"kind":"primitive","name":"Int"}}`),
    ).toThrowError(/Number.*String.*Boolean.*Null/);
  });

  it("rejects unknown predicate name in strict mode but accepts in non-strict", () => {
    const json = `[{"kind":"property","name":"p","formula":{"kind":"atomic","predicate":"???","args":[]}}]`;
    expect(() => parseDocument(json, { strict: true })).toThrowError(/canonical predicate/);
    expect(() => parseDocument(json, { strict: false })).not.toThrow();
  });

  it("rejects non-canonical primitive sort name in strict mode but accepts in non-strict", () => {
    const json = `{"kind":"primitive","name":"Frob"}`;
    expect(() => parseSort(json, { strict: true })).toThrowError(/Bool.*Int/);
    expect(() => parseSort(json, { strict: false })).not.toThrow();
  });

  it("rejects out-of-order keys in strict mode but accepts in non-strict", () => {
    // strict order is kind, name, formula. Reorder to name first.
    const json = `[{"name":"x","kind":"property","formula":{"kind":"atomic","predicate":"true","args":[]}}]`;
    expect(() => parseDocument(json, { strict: true })).toThrowError(/keys in order/);
    expect(() => parseDocument(json, { strict: false })).not.toThrow();
  });

  it("rejects extra optional keys not in the optional-keys list", () => {
    const json = `[{"kind":"bridge","name":"b","sourceSymbol":"s","sourceLayer":"l","targetContractCid":"c","targetLayer":"t","mystery":1}]`;
    expect(() => parseDocument(json)).toThrowError(/unexpected key/);
  });

  it("rejects var term with array name", () => {
    expect(() =>
      parseTerm(`{"kind":"var","name":[],"sort":{"kind":"primitive","name":"Int"}}`),
    ).toThrowError(/string var name/);
  });

  it("rejects atomic formula whose args is not an array", () => {
    expect(() =>
      parseFormula(`{"kind":"atomic","predicate":"=","args":"nope"}`),
    ).toThrowError(/JSON array/);
  });

  it("rejects lambda whose kind is not 'lambda'", () => {
    expect(() =>
      parseFormula(
        `{"kind":"forall","sort":{"kind":"primitive","name":"Int"},"predicate":{"kind":"NOT-LAMBDA","varName":"x","sort":{"kind":"primitive","name":"Int"},"body":{"kind":"atomic","predicate":"true","args":[]}}}`,
      ),
    ).toThrowError(/"lambda"/);
  });
});

// ---------------------------------------------------------------------------
// 4. Error structure — error includes path, expected, actual
// ---------------------------------------------------------------------------

describe("grammar parser — error structure", () => {
  it("populates path, expected, actual on the GrammarParseError", () => {
    let caught: GrammarParseError | null = null;
    try {
      parseDocument(`[{"kind":"property","name":42,"formula":{"kind":"atomic","predicate":"true","args":[]}}]`);
    } catch (e) {
      caught = e as GrammarParseError;
    }
    expect(caught).toBeInstanceOf(GrammarParseError);
    expect(caught?.path).toBe("/0/name");
    expect(caught?.expected).toMatch(/string/);
    expect(caught?.actual).toBe(42);
  });
});

// ---------------------------------------------------------------------------
// 5. Empty operands (and/or with zero operands)
// ---------------------------------------------------------------------------

describe("grammar parser — boundary empties", () => {
  it("accepts and with zero conjuncts", () => {
    const f = parseFormula(`{"kind":"and","conjuncts":[]}`);
    expect(f).toEqual({ kind: "and", conjuncts: [] });
  });

  it("accepts or with zero disjuncts", () => {
    const f = parseFormula(`{"kind":"or","disjuncts":[]}`);
    expect(f).toEqual({ kind: "or", disjuncts: [] });
  });

  it("emits and round-trips empty and / or formulas", () => {
    const text = emitDocument([
      {
        kind: "property",
        name: "p",
        formula: {
          kind: "and",
          conjuncts: [{ kind: "or", disjuncts: [] }],
        },
      },
    ]);
    expect(parseDocument(text, { strict: true })).toEqual([
      {
        kind: "property",
        name: "p",
        formula: { kind: "and", conjuncts: [{ kind: "or", disjuncts: [] }] },
      },
    ]);
  });
});

// ---------------------------------------------------------------------------
// 6. Seam test — parser output flows correctly into the canonicalizer
//
// The grammar describes the kit-emit JSON layer. The canonicalizer is the
// next layer in (fixture JSON → IR → propertyHash). This test asserts that
// IR produced by the parser hashes identically to IR produced in-process —
// catching any silent drift in field types or kind discriminators that
// .toEqual() might miss when both sides of the comparison are wrong in the
// same way.
// ---------------------------------------------------------------------------

describe("grammar parser — propertyHash seam", () => {
  beforeEach(() => {
    _resetCollector();
  });

  it("propertyHashFromFormula(parsed) === propertyHashFromFormula(in-process)", () => {
    // Build the same formula two ways: in-process via the symbolic kit, and
    // by parsing the kit's JSON output.
    const finish = beginCollecting();
    property("seam-test", forAll(Int, (x) => gt(x, num(0))));
    const inProcessDecls = finish();
    const inFirst = inProcessDecls[0];
    if (!inFirst || inFirst.kind !== "property") throw new Error("expected property decl");

    const json = emitDocument(inProcessDecls);
    const parsedDecls = parseDocument(json, { strict: true });
    const parsedFirst = parsedDecls[0];
    if (!parsedFirst || parsedFirst.kind !== "property") {
      throw new Error("expected property decl");
    }

    expect(propertyHashFromFormula(parsedFirst.formula)).toBe(
      propertyHashFromFormula(inFirst.formula),
    );
  });
});
