/**
 * Tests for the IR library.
 *
 * Type-dialect brand rejection by tsserver is NOT directly exercised by
 * vitest (vitest does not run tsc). See the doc-test comment block at
 * the bottom of this file for the type-system check and how to verify it.
 */

import { describe, it, expect, beforeEach } from "vitest";

import {
  Bool,
  Int,
  Real,
  Ref,
  SetOf,
  TupleOf,
  FuncOf,
} from "./sorts.js";

import {
  nonZero,
  assertNonZero,
  nonEmpty,
  assertNonEmpty,
  sorted,
  assertSorted,
  nonNull,
  assertNonNull,
  refined,
  range,
  assertRange,
} from "./brands.js";

import { forAll, exists, forSome, _resetCounter } from "./quantifiers.js";
import { and, or, not, implies, iff } from "./connectives.js";
import { assert } from "./assert.js";
import { property } from "./property.js";
import type { IrFormula, IrTerm } from "./formulas.js";

// ---------------------------------------------------------------------------
// Reset the variable counter before each test for deterministic names.
// ---------------------------------------------------------------------------
beforeEach(() => {
  _resetCounter();
});

// ---------------------------------------------------------------------------
// Sorts
// ---------------------------------------------------------------------------

describe("sorts", () => {
  it("primitive sorts are correctly shaped", () => {
    expect(Int).toEqual({ kind: "primitive", name: "Int" });
    expect(Bool).toEqual({ kind: "primitive", name: "Bool" });
    expect(Real).toEqual({ kind: "primitive", name: "Real" });
    expect(Ref).toEqual({ kind: "primitive", name: "Ref" });
  });

  it("SetOf constructs a set sort", () => {
    expect(SetOf(Int)).toEqual({ kind: "set", element: { kind: "primitive", name: "Int" } });
  });

  it("TupleOf constructs a tuple sort", () => {
    expect(TupleOf(Int, Bool)).toEqual({
      kind: "tuple",
      elements: [
        { kind: "primitive", name: "Int" },
        { kind: "primitive", name: "Bool" },
      ],
    });
  });

  it("FuncOf constructs a function sort", () => {
    expect(FuncOf([Int], Bool)).toEqual({
      kind: "function",
      domain: [{ kind: "primitive", name: "Int" }],
      range: { kind: "primitive", name: "Bool" },
    });
  });
});

// ---------------------------------------------------------------------------
// Brands — construction
// ---------------------------------------------------------------------------

describe("brands — nonZero", () => {
  it("nonZero returns value for non-zero input", () => {
    expect(nonZero(5)).toBe(5);
    expect(nonZero(-1)).toBe(-1);
    expect(nonZero(0.5)).toBe(0.5);
  });

  it("nonZero returns null for zero", () => {
    expect(nonZero(0)).toBeNull();
  });

  it("assertNonZero returns value for non-zero input", () => {
    expect(assertNonZero(3)).toBe(3);
  });

  it("assertNonZero throws for zero", () => {
    expect(() => assertNonZero(0)).toThrow("assertNonZero");
  });
});

describe("brands — nonEmpty", () => {
  it("nonEmpty returns array for non-empty input", () => {
    const arr = [1, 2, 3];
    expect(nonEmpty(arr)).toBe(arr);
  });

  it("nonEmpty returns null for empty array", () => {
    expect(nonEmpty([])).toBeNull();
  });

  it("assertNonEmpty throws for empty array", () => {
    expect(() => assertNonEmpty([])).toThrow("assertNonEmpty");
  });
});

describe("brands — sorted", () => {
  it("sorted wraps without checking", () => {
    const arr = [3, 1, 2]; // intentionally unsorted
    expect(sorted(arr)).toBe(arr);
  });

  it("assertSorted succeeds for sorted array", () => {
    expect(assertSorted([1, 2, 3])).toEqual([1, 2, 3]);
  });

  it("assertSorted throws for unsorted array", () => {
    expect(() => assertSorted([3, 1, 2])).toThrow("assertSorted");
  });
});

describe("brands — nonNull", () => {
  it("nonNull returns value when not null", () => {
    expect(nonNull(42)).toBe(42);
    expect(nonNull("hello")).toBe("hello");
  });

  it("nonNull returns null for null", () => {
    expect(nonNull(null)).toBeNull();
  });

  it("nonNull returns null for undefined", () => {
    expect(nonNull(undefined)).toBeNull();
  });

  it("assertNonNull throws for null", () => {
    expect(() => assertNonNull(null)).toThrow("assertNonNull");
  });
});

describe("brands — refined and range", () => {
  it("refined wraps the value identity", () => {
    const x = refined(42, "is positive integer");
    expect(x).toBe(42);
  });

  it("range wraps the value identity (trust-the-caller)", () => {
    const x = range(5, 0, 10);
    expect(x).toBe(5);
  });

  it("assertRange succeeds within bounds", () => {
    expect(assertRange(5, 0, 10)).toBe(5);
  });

  it("assertRange throws outside bounds", () => {
    expect(() => assertRange(11, 0, 10)).toThrow("assertRange");
    expect(() => assertRange(-1, 0, 10)).toThrow("assertRange");
  });
});

// ---------------------------------------------------------------------------
// forAll
// ---------------------------------------------------------------------------

describe("forAll", () => {
  it("constructs a forall node", () => {
    const f = forAll(Int, (x) => assert.notEqual(x, 0));
    expect(f.kind).toBe("forall");
  });

  it("stores the sort", () => {
    const f = forAll(Int, (x) => assert.notEqual(x, 0));
    if (f.kind !== "forall") throw new Error("expected forall");
    expect(f.sort).toEqual(Int);
  });

  it("predicate is a lambda with the variable name and evaluated body", () => {
    const f = forAll(Int, (x) => assert.notEqual(x, 0));
    if (f.kind !== "forall") throw new Error("expected forall");
    expect(f.predicate.kind).toBe("lambda");
    expect(f.predicate.varName).toBe("_x0");
    expect(f.predicate.body.kind).toBe("atomic");
  });

  it("body receives a var IrTerm with the correct sort", () => {
    let capturedTerm: IrTerm | null = null;
    forAll(Int, (x) => {
      capturedTerm = x;
      return assert.notEqual(x, 0);
    });
    expect(capturedTerm).not.toBeNull();
    expect((capturedTerm as unknown as IrTerm).kind).toBe("var");
    expect((capturedTerm as unknown as IrTerm).sort).toEqual(Int);
  });

  it("generates distinct variable names for nested quantifiers", () => {
    const f = forAll(Int, (_x) =>
      exists(Int, (_y) => assert.lessThan(_x, _y)),
    );
    if (f.kind !== "forall") throw new Error();
    if (f.predicate.body.kind !== "exists") throw new Error();
    expect(f.predicate.varName).toBe("_x0");
    expect(f.predicate.body.predicate.varName).toBe("_x1");
  });
});

// ---------------------------------------------------------------------------
// exists
// ---------------------------------------------------------------------------

describe("exists", () => {
  it("constructs an exists node", () => {
    const f = exists(Ref, (x) => assert.kindOf(x, "sanitize"));
    expect(f.kind).toBe("exists");
  });

  it("stores the sort and lambda", () => {
    const f = exists(Ref, (x) => assert.kindOf(x, "sanitize"));
    if (f.kind !== "exists") throw new Error();
    expect(f.sort).toEqual(Ref);
    expect(f.predicate.kind).toBe("lambda");
  });
});

// ---------------------------------------------------------------------------
// forSome
// ---------------------------------------------------------------------------

describe("forSome", () => {
  it("constructs a bounded exists node wrapping member + body", () => {
    const domainTerm: IrTerm = { kind: "var", name: "S", sort: SetOf(Int) };
    const f = forSome(domainTerm, Int, (x) => assert.greaterThan(x, 0));
    expect(f.kind).toBe("exists");
    if (f.kind !== "exists") throw new Error();
    expect(f.predicate.body.kind).toBe("and");
    if (f.predicate.body.kind !== "and") throw new Error();
    const [memberFormula] = f.predicate.body.conjuncts;
    expect(memberFormula.kind).toBe("atomic");
    if (memberFormula.kind !== "atomic") throw new Error();
    expect(memberFormula.predicate).toBe("member");
  });
});

// ---------------------------------------------------------------------------
// Connectives
// ---------------------------------------------------------------------------

describe("connectives", () => {
  const a: IrFormula = assert.equal(0, 0);
  const b: IrFormula = assert.equal(1, 1);

  it("and constructs conjunction", () => {
    const f = and(a, b);
    expect(f.kind).toBe("and");
    if (f.kind !== "and") throw new Error();
    expect(f.conjuncts).toHaveLength(2);
  });

  it("and with single arg returns the arg", () => {
    expect(and(a)).toBe(a);
  });

  it("and with no args returns true atomic", () => {
    const f = and();
    expect(f.kind).toBe("atomic");
    if (f.kind !== "atomic") throw new Error();
    expect(f.predicate).toBe("true");
  });

  it("or constructs disjunction", () => {
    const f = or(a, b);
    expect(f.kind).toBe("or");
    if (f.kind !== "or") throw new Error();
    expect(f.disjuncts).toHaveLength(2);
  });

  it("not constructs negation", () => {
    const f = not(a);
    expect(f.kind).toBe("not");
    if (f.kind !== "not") throw new Error();
    expect(f.body).toBe(a);
  });

  it("implies constructs implication", () => {
    const f = implies(a, b);
    expect(f.kind).toBe("implies");
    if (f.kind !== "implies") throw new Error();
    expect(f.antecedent).toBe(a);
    expect(f.consequent).toBe(b);
  });

  it("iff constructs biconditional", () => {
    const f = iff(a, b);
    expect(f.kind).toBe("iff");
    if (f.kind !== "iff") throw new Error();
    expect(f.left).toBe(a);
    expect(f.right).toBe(b);
  });
});

// ---------------------------------------------------------------------------
// assert namespace
// ---------------------------------------------------------------------------

describe("assert namespace", () => {
  const x: IrTerm = { kind: "var", name: "x", sort: Int };
  const y: IrTerm = { kind: "var", name: "y", sort: Int };

  it("notEqual constructs atomic ≠", () => {
    const f = assert.notEqual(x, y);
    expect(f.kind).toBe("atomic");
    if (f.kind !== "atomic") throw new Error();
    expect(f.predicate).toBe("≠");
    expect(f.args).toHaveLength(2);
  });

  it("equal constructs atomic =", () => {
    const f = assert.equal(x, y);
    if (f.kind !== "atomic") throw new Error();
    expect(f.predicate).toBe("=");
  });

  it("lessThan constructs atomic <", () => {
    const f = assert.lessThan(x, y);
    if (f.kind !== "atomic") throw new Error();
    expect(f.predicate).toBe("<");
  });

  it("greaterThanOrEqual constructs atomic ≥", () => {
    const f = assert.greaterThanOrEqual(x, y);
    if (f.kind !== "atomic") throw new Error();
    expect(f.predicate).toBe("≥");
  });

  it("primitives are auto-lifted to const terms", () => {
    const f = assert.notEqual(x, 0);
    if (f.kind !== "atomic") throw new Error();
    expect(f.args[1]).toEqual({ kind: "const", value: 0, sort: Int });
  });

  it("string primitives are lifted with String sort", () => {
    const f = assert.equal(x, "hello");
    if (f.kind !== "atomic") throw new Error();
    expect(f.args[1].sort).toEqual({ kind: "primitive", name: "String" });
  });

  it("kindOf constructs kind-of atomic", () => {
    const node: IrTerm = { kind: "var", name: "n", sort: { kind: "primitive", name: "Node" } };
    const f = assert.kindOf(node, "execSync");
    if (f.kind !== "atomic") throw new Error();
    expect(f.predicate).toBe("kind-of");
    expect(f.args[1]).toEqual({ kind: "const", value: "execSync", sort: { kind: "primitive", name: "String" } });
  });

  it("dataFlowsTo constructs data-flows-to atomic", () => {
    const f = assert.dataFlowsTo(x, y);
    if (f.kind !== "atomic") throw new Error();
    expect(f.predicate).toBe("data-flows-to");
  });

  it("transitionFrom().to() constructs transition-from-to atomic", () => {
    const f = assert.transitionFrom(x).to(y);
    if (f.kind !== "atomic") throw new Error();
    expect(f.predicate).toBe("transition-from-to");
    expect(f.args).toHaveLength(2);
  });

  it("assert.true and assert.false work as methods", () => {
    const t = assert.true(x);
    if (t.kind !== "atomic") throw new Error();
    expect(t.predicate).toBe("true");

    const ff = assert.false(x);
    if (ff.kind !== "atomic") throw new Error();
    expect(ff.predicate).toBe("false");
  });

  it("subset constructs subset atomic", () => {
    const s: IrTerm = { kind: "var", name: "S", sort: SetOf(Int) };
    const f = assert.subset(s, s);
    if (f.kind !== "atomic") throw new Error();
    expect(f.predicate).toBe("subset");
  });

  it("member constructs member atomic", () => {
    const s: IrTerm = { kind: "var", name: "S", sort: SetOf(Int) };
    const f = assert.member(x, s);
    if (f.kind !== "atomic") throw new Error();
    expect(f.predicate).toBe("member");
  });
});

// ---------------------------------------------------------------------------
// property()
// ---------------------------------------------------------------------------

describe("property()", () => {
  it("produces a Property struct with the right shape", () => {
    const p = property({
      name: "denominator-nonzero",
      scope: { kind: "function", name: "divide" },
      bindings: { b: Int },
      formula: ({ b }) => assert.notEqual(b, 0),
    });

    expect(p.name).toBe("denominator-nonzero");
    expect(p.scope).toEqual({ kind: "function", name: "divide" });
    expect(p.bindings).toEqual({ b: Int });
    expect(p.formula.kind).toBe("atomic");
    if (p.formula.kind !== "atomic") throw new Error();
    expect(p.formula.predicate).toBe("≠");
  });

  it("passes IrTerm handles with correct sort and name", () => {
    let capturedTerm: IrTerm | null = null;
    property({
      name: "test",
      scope: { kind: "function", name: "f" },
      bindings: { b: Int },
      formula: ({ b }) => {
        capturedTerm = b;
        return assert.notEqual(b, 0);
      },
    });
    expect(capturedTerm).not.toBeNull();
    const t = capturedTerm as unknown as Extract<IrTerm, { kind: "var" }>;
    expect(t.kind).toBe("var");
    expect(t.name).toBe("b");
    expect(t.sort).toEqual(Int);
  });

  it("accepts a plain IrFormula (non-function formula)", () => {
    const formula: IrFormula = assert.equal(0, 0);
    const p = property({
      name: "test",
      scope: { kind: "module", path: "api" },
      bindings: {},
      formula,
    });
    expect(p.formula).toBe(formula);
  });

  it("includes hint when provided", () => {
    const p = property({
      name: "test",
      scope: { kind: "function", name: "f" },
      bindings: {},
      formula: assert.equal(0, 0),
      hint: "requires-smt",
    });
    expect(p.hint).toBe("requires-smt");
  });

  it("omits hint when not provided", () => {
    const p = property({
      name: "test",
      scope: { kind: "function", name: "f" },
      bindings: {},
      formula: assert.equal(0, 0),
    });
    expect(Object.prototype.hasOwnProperty.call(p, "hint")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// JSON round-trip
// ---------------------------------------------------------------------------

describe("IrFormula round-trip through JSON", () => {
  it("atomic formula survives JSON parse/stringify", () => {
    const f: IrFormula = assert.notEqual(
      { kind: "var", name: "b", sort: Int },
      0,
    );
    const roundTripped = JSON.parse(JSON.stringify(f)) as IrFormula;
    expect(roundTripped).toEqual(f);
  });

  it("forall formula survives JSON parse/stringify", () => {
    const f = forAll(Int, (x) => assert.greaterThan(x, 0));
    const roundTripped = JSON.parse(JSON.stringify(f)) as IrFormula;
    expect(roundTripped).toEqual(f);
  });

  it("nested formula survives JSON parse/stringify", () => {
    const f = forAll(Int, (x) =>
      implies(
        assert.greaterThan(x, 0),
        exists(Int, (y) => and(assert.lessThan(x, y), assert.notEqual(y, 0))),
      ),
    );
    const roundTripped = JSON.parse(JSON.stringify(f)) as IrFormula;
    expect(roundTripped).toEqual(f);
  });

  it("property formula survives JSON parse/stringify", () => {
    const p = property({
      name: "taint-check",
      scope: { kind: "module", path: "api" },
      bindings: { input: Ref, sink: Ref },
      formula: ({ input, sink }) =>
        implies(
          assert.dataFlowsTo(input, sink),
          exists(Ref, (path) =>
            and(assert.onPath(path, input, sink), assert.kindOf(path, "sanitize")),
          ),
        ),
    });
    const roundTripped = JSON.parse(JSON.stringify(p)) as typeof p;
    expect(roundTripped).toEqual(p);
  });
});

// ---------------------------------------------------------------------------
// Type-dialect: doc-test (verified by tsc, NOT by vitest)
// ---------------------------------------------------------------------------

/**
 * TYPE-DIALECT REJECTION TEST (doc-test — not a runtime assertion)
 *
 * The following shows how tsserver enforces the brand at compile time.
 * It is NOT exercised by `vitest run`. To verify, run `npx tsc --noEmit`
 * against this file and check that line A produces a TS error.
 *
 * ```typescript
 * import type { NonZero } from "./brands.js";
 *
 * function divide(a: number, b: NonZero<number>): number {
 *   return a / b;
 * }
 *
 * // Line A — tsc must produce an error here because `0` is not NonZero<number>.
 * // @ts-expect-error
 * divide(10, 0);
 *
 * // Line B — this must compile successfully.
 * import { assertNonZero } from "./brands.js";
 * divide(10, assertNonZero(5)); // ok: assertNonZero(5) returns NonZero<number>
 * ```
 *
 * Verification: `npx tsc --noEmit` should pass (the @ts-expect-error consumes
 * the error on Line A, confirming the error exists; Line B compiles clean).
 */
