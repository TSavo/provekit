/**
 * IR -> Lean 4 translator tests.
 *
 * Mirrors the SMT translator's coverage with stable string-equality checks
 * over the rendered Lean output. Tests assert the exact Lean source the
 * translator emits — not equivalence — so any unintended drift in render
 * shape surfaces as a failed test.
 */

import { describe, it, expect, beforeEach } from "vitest";

import { Bool, Int, Real, String as Str, SetOf } from "../sorts.js";
import { forAll, exists, _resetCounter } from "../quantifiers.js";
import { and, or, not, implies, iff } from "../connectives.js";
import { assert as A } from "../assert.js";
import type { IrFormula, IrTerm, Sort } from "../formulas.js";

import { emitLean, emitLeanTheorem, LeanUnsupportedError } from "./index.js";
import { emitSort, collectUserSorts, isBuiltInPrimitive } from "./sorts.js";
import {
  collectDeclarations,
  emitSortDeclarations,
  emitFunctionDeclarations,
} from "./declarations.js";
import { emitFormula } from "./emit.js";

beforeEach(() => {
  _resetCounter();
});

const cents: Sort = { kind: "primitive", name: "Cents" };

function ctor(name: string, args: IrTerm[], sort: Sort): IrTerm {
  const t: IrTerm = { kind: "ctor", name, args };
  Object.defineProperty(t, Symbol.for("provekit.ir.sortHint"), {
    value: sort,
    enumerable: false,
    writable: true,
    configurable: true,
  });
  return t;
}

// ---------------------------------------------------------------------------
// Per-node-kind emission
// ---------------------------------------------------------------------------

describe("emitLean — per node kind", () => {
  it("emits forall with sort and binder", () => {
    const f = forAll(Int, (x) => A.equal(x, x));
    expect(emitLean(f)).toBe("∀ (_x0 : Int), (_x0 = _x0)");
  });

  it("emits exists with sort and binder", () => {
    const f = exists(Int, (x) => A.greaterThan(x, 0));
    expect(emitLean(f)).toBe("∃ (_x0 : Int), (_x0 > 0)");
  });

  it("emits and with two conjuncts using ∧", () => {
    const f = and(A.equal(1, 1), A.equal(2, 2));
    expect(emitLean(f)).toBe("((1 = 1) ∧ (2 = 2))");
  });

  it("emits or with two disjuncts using ∨", () => {
    const f = or(A.equal(1, 1), A.equal(2, 2));
    expect(emitLean(f)).toBe("((1 = 1) ∨ (2 = 2))");
  });

  it("emits not using ¬", () => {
    const f = not(A.equal(1, 2));
    expect(emitLean(f)).toBe("(¬ (1 = 2))");
  });

  it("emits implies using →", () => {
    const f = implies(A.equal(1, 1), A.equal(2, 2));
    expect(emitLean(f)).toBe("((1 = 1) → (2 = 2))");
  });

  it("emits iff (desugared via library to two implications under and)", () => {
    const f = iff(A.equal(1, 1), A.equal(2, 2));
    expect(emitLean(f)).toBe(
      "(((1 = 1) → (2 = 2)) ∧ ((2 = 2) → (1 = 1)))",
    );
  });

  it("emits ≠ using Lean's ≠ operator", () => {
    const f = A.notEqual(1, 2);
    expect(emitLean(f)).toBe("(1 ≠ 2)");
  });

  it("emits ordering predicates with infix operators", () => {
    expect(emitLean(A.lessThan(1, 2))).toBe("(1 < 2)");
    expect(emitLean(A.lessThanOrEqual(1, 2))).toBe("(1 ≤ 2)");
    expect(emitLean(A.greaterThan(1, 2))).toBe("(1 > 2)");
    expect(emitLean(A.greaterThanOrEqual(1, 2))).toBe("(1 ≥ 2)");
  });

  it("emits ctor terms as Lean function application", () => {
    const x: IrTerm = { kind: "var", name: "_x0"};
    const f: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [ctor("parseInt", [x], Int), { kind: "const", value: 42, sort: Int }],
    };
    expect(emitLean(f)).toBe("((parseInt _x0) = 42)");
  });

  it("emits zero-arg ctor as bare identifier", () => {
    const f: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [ctor("ZERO", [], Int), { kind: "const", value: 0, sort: Int }],
    };
    expect(emitLean(f)).toBe("(ZERO = 0)");
  });
});

// ---------------------------------------------------------------------------
// Quantifier nesting + binder name preservation
// ---------------------------------------------------------------------------

describe("emitLean — quantifier scoping", () => {
  it("preserves distinct binder names under nesting", () => {
    const f = forAll(Int, (x) => forAll(Int, (y) => A.lessThan(x, y)));
    expect(emitLean(f)).toBe(
      "∀ (_x0 : Int), ∀ (_x1 : Int), (_x0 < _x1)",
    );
  });

  it("renames inner binder when names clash", () => {
    const inner: IrFormula = {
      kind: "forall", name: "x", sort: Int, body: {
          kind: "atomic",
          name: "=",
          args: [
            { kind: "var", name: "x"},
            { kind: "var", name: "x"},
          ],
        },
    };
    const outer: IrFormula = {
      kind: "forall", name: "x", sort: Int, body: inner,
    };
    const out = emitLean(outer);
    expect(out).toMatch(/^∀ \(x : Int\), ∀ \(x__d\d+ : Int\),/);
  });

  it("allows mixing forall and exists", () => {
    const f = forAll(Int, (x) => exists(Int, (y) => A.equal(x, y)));
    expect(emitLean(f)).toBe(
      "∀ (_x0 : Int), ∃ (_x1 : Int), (_x0 = _x1)",
    );
  });
});

// ---------------------------------------------------------------------------
// Sort mapping + structured errors
// ---------------------------------------------------------------------------

describe("emitLean — sort mapping", () => {
  it("maps Bool/Int/String to Lean built-ins", () => {
    expect(emitLean(forAll(Bool, () => A.equal(1, 1)))).toContain("Bool");
    expect(emitLean(forAll(Int, () => A.equal(1, 1)))).toContain("Int");
    expect(emitLean(forAll(Str, () => A.equal(1, 1)))).toContain("String");
  });

  it("renders user-branded sorts by name", () => {
    const f = forAll(cents, (x) => A.equal(x, x));
    expect(emitLean(f)).toBe("∀ (_x0 : Cents), (_x0 = _x0)");
  });

  it("throws LeanUnsupportedError for Set sort", () => {
    expect(() => emitLean(forAll(SetOf(Int), (s) => A.equal(s, s)))).toThrow(
      LeanUnsupportedError,
    );
  });

  it("throws LeanUnsupportedError for tuple sort", () => {
    const tupleSort: Sort = { kind: "tuple", elements: [Int, Bool] };
    const f: IrFormula = {
      kind: "forall", name: "_x0", sort: tupleSort, body: { kind: "atomic", name: "true", args: [] },
    };
    expect(() => emitLean(f)).toThrow(LeanUnsupportedError);
  });

  it("throws LeanUnsupportedError for function sort", () => {
    const fnSort: Sort = { kind: "function", domain: [Int], range: Bool };
    const f: IrFormula = {
      kind: "forall", name: "_x0", sort: fnSort, body: { kind: "atomic", name: "true", args: [] },
    };
    expect(() => emitLean(f)).toThrow(LeanUnsupportedError);
  });
});

// ---------------------------------------------------------------------------
// Constant literals
// ---------------------------------------------------------------------------

describe("emitLean — constants", () => {
  it("renders integer literals", () => {
    expect(emitLean(A.equal(1, 1))).toBe("(1 = 1)");
  });

  it("renders negative integers with Int annotation", () => {
    expect(emitLean(A.equal(-3, -3))).toBe("((-3 : Int) = (-3 : Int))");
  });

  it("renders booleans", () => {
    const lhs: IrTerm = { kind: "const", value: true, sort: Bool };
    const rhs: IrTerm = { kind: "const", value: false, sort: Bool };
    const f: IrFormula = { kind: "atomic", name: "=", args: [lhs, rhs] };
    expect(emitLean(f)).toBe("(true = false)");
  });

  it("escapes quotes in string literals using backslash escapes", () => {
    const f: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [
        { kind: "const", value: 'a"b', sort: Str },
        { kind: "const", value: "c", sort: Str },
      ],
    };
    expect(emitLean(f)).toBe('("a\\"b" = "c")');
  });

  it("renders bigint literals", () => {
    const f: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [
        { kind: "const", value: 100n, sort: Int },
        { kind: "const", value: 100n, sort: Int },
      ],
    };
    expect(emitLean(f)).toBe("(100 = 100)");
  });

  it("renders negative bigint with Int annotation", () => {
    const f: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [
        { kind: "const", value: -100n, sort: Int },
        { kind: "const", value: -100n, sort: Int },
      ],
    };
    expect(emitLean(f)).toBe("((-100 : Int) = (-100 : Int))");
  });
});

// ---------------------------------------------------------------------------
// Theorem emission
// ---------------------------------------------------------------------------

describe("emitLeanTheorem", () => {
  it("emits a theorem with a deterministic name and sorry proof", () => {
    const out = emitLeanTheorem({
      axioms: [],
      assertion: forAll(Int, (x) => A.equal(x, x)),
    });
    expect(out.theoremName).toMatch(/^prop_[0-9a-f]{16}$/);
    expect(out.source).toContain(`theorem ${out.theoremName}`);
    expect(out.source).toContain("∀ (_x0 : Int), (_x0 = _x0)");
    expect(out.source).toContain("sorry");
  });

  it("derives the same name for the same formula (deterministic)", () => {
    const f = forAll(Int, (x) => A.equal(x, x));
    const a = emitLeanTheorem({ axioms: [], assertion: f });
    _resetCounter();
    const b = emitLeanTheorem({ axioms: [], assertion: forAll(Int, (x) => A.equal(x, x)) });
    expect(a.theoremName).toBe(b.theoremName);
  });

  it("respects an explicitly supplied theorem name", () => {
    const out = emitLeanTheorem({
      axioms: [],
      assertion: A.equal(1, 1),
      name: "my_theorem",
    });
    expect(out.theoremName).toBe("my_theorem");
    expect(out.source).toContain("theorem my_theorem");
  });

  it("folds axioms into the statement as implications", () => {
    const ax = forAll(Str, (s) =>
      A.greaterThanOrEqual(ctor("parseInt", [s], Int), 0),
    );
    const claim = A.greaterThanOrEqual(
      ctor("parseInt", [{ kind: "const", value: "abc", sort: Str }], Int),
      0,
    );
    const out = emitLeanTheorem({ axioms: [ax], assertion: claim });
    expect(out.source).toContain("axiom parseInt : String -> Int");
    expect(out.source).toContain("∀ (_x0 : String), ((parseInt _x0) ≥ 0)");
    expect(out.source).toContain("→");
    expect(out.source).toContain('((parseInt "abc") ≥ 0)');
  });

  it("declares user sorts and uninterpreted predicates as axioms", () => {
    const x: IrTerm = { kind: "var", name: "_x0"};
    const claim: IrFormula = {
      kind: "forall", name: "_x0", sort: Int, body: { kind: "atomic", name: "isPrime", args: [x] },
    };
    const out = emitLeanTheorem({ axioms: [], assertion: claim });
    expect(out.source).toContain("axiom isPrime : Int -> Prop");
    expect(out.source).toContain("(isPrime _x0)");
  });

  it("declares user-defined sorts before the theorem statement", () => {
    const claim = forAll(cents, (x) => A.equal(x, x));
    const out = emitLeanTheorem({ axioms: [], assertion: claim });
    expect(out.source).toContain("axiom Cents : Type");
    const sortIdx = out.source.indexOf("axiom Cents : Type");
    const theoremIdx = out.source.indexOf(`theorem ${out.theoremName}`);
    expect(theoremIdx).toBeGreaterThan(sortIdx);
  });
});

// ---------------------------------------------------------------------------
// sorts module — direct API
// ---------------------------------------------------------------------------

describe("lean/sorts — emitSort", () => {
  it("maps each built-in primitive name correctly", () => {
    expect(emitSort(Int)).toBe("Int");
    expect(emitSort(Bool)).toBe("Bool");
    expect(emitSort(Str)).toBe("String");
  });

  it("emits user primitives as bare identifiers", () => {
    expect(emitSort(cents)).toBe("Cents");
  });

  it("Real maps to Real (Mathlib symbol — emitted as identifier; documented divergence)", () => {
    // The current sorts table maps Real -> "Real". Plain Lean has no Real;
    // a kit using Real must either depend on Mathlib or shadow it.
    expect(emitSort(Real)).toBe("Real");
  });
});

describe("lean/sorts — collectUserSorts", () => {
  it("ignores built-in sorts", () => {
    const out = new Set<string>();
    collectUserSorts(Int, out);
    collectUserSorts(Bool, out);
    expect(out.size).toBe(0);
  });

  it("collects user-defined primitives", () => {
    const out = new Set<string>();
    const ux: Sort = { kind: "primitive", name: "USort" };
    collectUserSorts(ux, out);
    expect([...out]).toEqual(["USort"]);
  });
});

describe("lean/sorts — isBuiltInPrimitive", () => {
  it("recognizes built-ins", () => {
    expect(isBuiltInPrimitive("Int")).toBe(true);
    expect(isBuiltInPrimitive("Bool")).toBe(true);
    expect(isBuiltInPrimitive("String")).toBe(true);
  });

  it("rejects non-built-in names", () => {
    expect(isBuiltInPrimitive("Cents")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// declarations module — direct API
// ---------------------------------------------------------------------------

describe("lean/declarations — collectDeclarations", () => {
  it("collects user sorts from quantifier sorts", () => {
    const f = forAll(cents, (x) => A.equal(x, x));
    const decls = collectDeclarations([f]);
    expect(decls.userSorts).toEqual(["Cents"]);
  });

  it("collects ctor signatures from atomic args", () => {
    const x: IrTerm = { kind: "var", name: "_x0"};
    const f: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [ctor("parseInt", [x], Int), { kind: "const", value: 42, sort: Int }],
    };
    const decls = collectDeclarations([f]);
    expect(decls.ctors.map((c) => c.name)).toContain("parseInt");
  });

  it("collects uninterpreted predicates and skips built-ins", () => {
    const x: IrTerm = { kind: "var", name: "_x0"};
    const f: IrFormula = {
      kind: "forall", name: "_x0", sort: Int, body: { kind: "atomic", name: "isPrime", args: [x] },
    };
    const decls = collectDeclarations([f]);
    expect(decls.predicates.map((p) => p.name)).toEqual(["isPrime"]);
  });
});

describe("lean/declarations — emitters", () => {
  it("emitSortDeclarations renders one axiom-of-Type line per user sort", () => {
    const decls = collectDeclarations([forAll(cents, (x) => A.equal(x, x))]);
    expect(emitSortDeclarations(decls)).toEqual(["axiom Cents : Type"]);
  });

  it("emitFunctionDeclarations renders ctor and predicate Prop axioms", () => {
    const x: IrTerm = { kind: "var", name: "_x0"};
    const f: IrFormula = {
      kind: "forall", name: "_x0", sort: Int, body: { kind: "atomic", name: "isPrime", args: [x] },
    };
    const decls = collectDeclarations([f]);
    const lines = emitFunctionDeclarations(decls);
    expect(lines).toContain("axiom isPrime : Int -> Prop");
  });
});

// ---------------------------------------------------------------------------
// emit module re-export sanity
// ---------------------------------------------------------------------------

describe("emitFormula (direct emit module)", () => {
  it("matches emitLean output", () => {
    const f = and(A.equal(1, 1), A.equal(2, 2));
    expect(emitFormula(f)).toBe(emitLean(f));
  });

  it("renders zero-conjunct and as 'True'", () => {
    expect(emitFormula({ kind: "and", operands: [] })).toBe("True");
  });

  it("renders zero-disjunct or as 'False'", () => {
    expect(emitFormula({ kind: "or", operands: [] })).toBe("False");
  });
});

// ---------------------------------------------------------------------------
// Structural sanity
// ---------------------------------------------------------------------------

describe("emitLean — structural sanity", () => {
  it("output has balanced parentheses", () => {
    const f = forAll(Int, (x) =>
      exists(Int, (y) => and(A.lessThan(x, y), implies(A.equal(x, 0), A.equal(y, 1)))),
    );
    const s = emitLean(f);
    let depth = 0;
    for (const ch of s) {
      if (ch === "(") depth++;
      else if (ch === ")") depth--;
      expect(depth).toBeGreaterThanOrEqual(0);
    }
    expect(depth).toBe(0);
  });

  it("emitLeanTheorem source ends with newline after sorry", () => {
    const out = emitLeanTheorem({ axioms: [], assertion: A.equal(1, 1) });
    expect(out.source.endsWith("\n")).toBe(true);
    expect(out.source.trimEnd().endsWith("sorry")).toBe(true);
  });
});

describe("emitLean — null constant rejection", () => {
  it("throws when emitting a null const", () => {
    const f: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [
        { kind: "const", value: null, sort: Int },
        { kind: "const", value: 0, sort: Int },
      ],
    };
    expect(() => emitLean(f)).toThrow(/null/);
  });
});

describe("emitLean — true/false predicate args", () => {
  it("emits atomic 'true' with no args as the literal True", () => {
    const f: IrFormula = { kind: "atomic", name: "true", args: [] };
    expect(emitLean(f)).toBe("True");
  });

  it("emits atomic 'false' with no args as the literal False", () => {
    const f: IrFormula = { kind: "atomic", name: "false", args: [] };
    expect(emitLean(f)).toBe("False");
  });
});
