/**
 * Tests for the IR → SMT-LIB translator.
 */

import { describe, it, expect, beforeEach } from "vitest";

import { Bool, Int, Real, String as Str, SetOf } from "../sorts.js";
import { forAll, exists, forSome, _resetCounter } from "../quantifiers.js";
import { and, or, not, implies, iff } from "../connectives.js";
import { assert as A } from "../assert.js";
import type { IrFormula, IrTerm, Sort } from "../formulas.js";

import { emitSmtLib, emitSmtLibProblem } from "./index.js";

beforeEach(() => {
  _resetCounter();
});

const cents: Sort = { kind: "primitive", name: "Cents" };

function ctor(name: string, args: IrTerm[], sort: Sort): IrTerm {
  return { kind: "ctor", name, args, sort };
}

// ---------------------------------------------------------------------------
// Per-node-kind emission
// ---------------------------------------------------------------------------

describe("emitSmtLib — per node kind", () => {
  it("emits forall with sort and binder", () => {
    const f = forAll(Int, (x) => A.equal(x, x));
    expect(emitSmtLib(f)).toBe("(forall ((_x0 Int)) (= _x0 _x0))");
  });

  it("emits exists with sort and binder", () => {
    const f = exists(Int, (x) => A.greaterThan(x, 0));
    expect(emitSmtLib(f)).toBe("(exists ((_x0 Int)) (> _x0 0))");
  });

  it("emits and with two conjuncts", () => {
    const f = and(A.equal(1, 1), A.equal(2, 2));
    expect(emitSmtLib(f)).toBe("(and (= 1 1) (= 2 2))");
  });

  it("emits or with two disjuncts", () => {
    const f = or(A.equal(1, 1), A.equal(2, 2));
    expect(emitSmtLib(f)).toBe("(or (= 1 1) (= 2 2))");
  });

  it("emits not", () => {
    const f = not(A.equal(1, 2));
    expect(emitSmtLib(f)).toBe("(not (= 1 2))");
  });

  it("emits implies as =>", () => {
    const f = implies(A.equal(1, 1), A.equal(2, 2));
    expect(emitSmtLib(f)).toBe("(=> (= 1 1) (= 2 2))");
  });

  it("emits iff (desugared to two implies under and)", () => {
    const f = iff(A.equal(1, 1), A.equal(2, 2));
    expect(emitSmtLib(f)).toBe(
      "(and (=> (= 1 1) (= 2 2)) (=> (= 2 2) (= 1 1)))",
    );
  });

  it("emits ≠ as distinct", () => {
    const f = A.notEqual(1, 2);
    expect(emitSmtLib(f)).toBe("(distinct 1 2)");
  });

  it("emits ordering predicates with SMT-LIB operators", () => {
    expect(emitSmtLib(A.lessThan(1, 2))).toBe("(< 1 2)");
    expect(emitSmtLib(A.lessThanOrEqual(1, 2))).toBe("(<= 1 2)");
    expect(emitSmtLib(A.greaterThan(1, 2))).toBe("(> 1 2)");
    expect(emitSmtLib(A.greaterThanOrEqual(1, 2))).toBe("(>= 1 2)");
  });

  it("emits ctor terms as application", () => {
    const x: IrTerm = { kind: "var", name: "_x0", sort: Str };
    const f: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [ctor("parseInt", [x], Int), { kind: "const", value: 42, sort: Int }],
    };
    expect(emitSmtLib(f)).toBe("(= (parseInt _x0) 42)");
  });

  it("emits zero-arg ctor as bare identifier", () => {
    const f: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [ctor("ZERO", [], Int), { kind: "const", value: 0, sort: Int }],
    };
    expect(emitSmtLib(f)).toBe("(= ZERO 0)");
  });
});

// ---------------------------------------------------------------------------
// Quantifier nesting + binder name preservation
// ---------------------------------------------------------------------------

describe("emitSmtLib — quantifier scoping", () => {
  it("preserves distinct binder names under nesting", () => {
    const f = forAll(Int, (x) => forAll(Int, (y) => A.lessThan(x, y)));
    expect(emitSmtLib(f)).toBe(
      "(forall ((_x0 Int)) (forall ((_x1 Int)) (< _x0 _x1)))",
    );
  });

  it("renames inner binder when names clash", () => {
    // Hand-roll a clashing IR (the standard builders never produce this).
    const inner: IrFormula = {
      kind: "forall",
      sort: Int,
      predicate: {
        kind: "lambda",
        varName: "x",
        sort: Int,
        body: {
          kind: "atomic",
          predicate: "=",
          args: [
            { kind: "var", name: "x", sort: Int },
            { kind: "var", name: "x", sort: Int },
          ],
        },
      },
    };
    const outer: IrFormula = {
      kind: "forall",
      sort: Int,
      predicate: {
        kind: "lambda",
        varName: "x",
        sort: Int,
        body: inner,
      },
    };
    const out = emitSmtLib(outer);
    // Outer binder keeps name "x"; inner is renamed because "x" is in scope.
    expect(out).toMatch(/^\(forall \(\(x Int\)\) \(forall \(\(x\$\d+ Int\)\)/);
  });

  it("allows mixing forall and exists", () => {
    const f = forAll(Int, (x) => exists(Int, (y) => A.equal(x, y)));
    expect(emitSmtLib(f)).toBe(
      "(forall ((_x0 Int)) (exists ((_x1 Int)) (= _x0 _x1)))",
    );
  });
});

// ---------------------------------------------------------------------------
// Sort mapping
// ---------------------------------------------------------------------------

describe("emitSmtLib — sort mapping", () => {
  it("maps Bool/Int/Real/String to SMT-LIB built-ins", () => {
    expect(emitSmtLib(forAll(Bool, () => A.equal(1, 1)))).toContain("Bool");
    expect(emitSmtLib(forAll(Int, () => A.equal(1, 1)))).toContain("Int");
    expect(emitSmtLib(forAll(Real, () => A.equal(1, 1)))).toContain("Real");
    expect(emitSmtLib(forAll(Str, () => A.equal(1, 1)))).toContain("String");
  });

  it("renders user-branded sorts by name", () => {
    const f = forAll(cents, (x) => A.equal(x, x));
    expect(emitSmtLib(f)).toBe("(forall ((_x0 Cents)) (= _x0 _x0))");
  });

  it("renders set sorts as (Set T)", () => {
    const f = forAll(SetOf(Int), (s) => A.equal(s, s));
    expect(emitSmtLib(f)).toBe("(forall ((_x0 (Set Int))) (= _x0 _x0))");
  });
});

// ---------------------------------------------------------------------------
// Constant literals
// ---------------------------------------------------------------------------

describe("emitSmtLib — constants", () => {
  it("renders integer literals", () => {
    expect(emitSmtLib(A.equal(1, 1))).toBe("(= 1 1)");
  });

  it("renders negative integers as (- n)", () => {
    expect(emitSmtLib(A.equal(-3, -3))).toBe("(= (- 3) (- 3))");
  });

  it("renders booleans", () => {
    const lhs: IrTerm = { kind: "const", value: true, sort: Bool };
    const rhs: IrTerm = { kind: "const", value: false, sort: Bool };
    const f: IrFormula = { kind: "atomic", predicate: "=", args: [lhs, rhs] };
    expect(emitSmtLib(f)).toBe("(= true false)");
  });

  it("quotes string literals and doubles embedded quotes", () => {
    const f: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [
        { kind: "const", value: 'a"b', sort: Str },
        { kind: "const", value: "c", sort: Str },
      ],
    };
    expect(emitSmtLib(f)).toBe('(= "a""b" "c")');
  });

  it("renders Real integers with a decimal point", () => {
    const f: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [
        { kind: "const", value: 3, sort: Real },
        { kind: "const", value: -2, sort: Real },
      ],
    };
    expect(emitSmtLib(f)).toBe("(= 3.0 (- 2.0))");
  });
});

// ---------------------------------------------------------------------------
// Full problem emission
// ---------------------------------------------------------------------------

describe("emitSmtLibProblem", () => {
  it("emits set-logic, declarations, axioms, negated assertion, check-sat", () => {
    // axiom: forall s:String, parseInt(s) ≥ 0
    const axiom = forAll(Str, (s) =>
      A.greaterThanOrEqual(ctor("parseInt", [s], Int), 0),
    );
    // assertion: parseInt("abc") ≥ 0
    const assertion = A.greaterThanOrEqual(
      ctor("parseInt", [{ kind: "const", value: "abc", sort: Str }], Int),
      0,
    );
    const out = emitSmtLibProblem({ axioms: [axiom], assertion });

    expect(out).toContain("(set-logic ALL)");
    expect(out).toContain("(declare-fun parseInt (String) Int)");
    expect(out).toContain("(assert (forall ((_x0 String)) (>= (parseInt _x0) 0)))");
    expect(out).toContain('(assert (not (>= (parseInt "abc") 0)))');
    expect(out).toContain("(check-sat)");
  });

  it("declares user sorts before functions", () => {
    const assertion = forAll(cents, (x) => A.equal(x, x));
    const out = emitSmtLibProblem({ axioms: [], assertion });

    const sortIdx = out.indexOf("(declare-sort Cents 0)");
    expect(sortIdx).toBeGreaterThan(0);
    // No declare-fun before the sort declaration.
    expect(out.slice(0, sortIdx)).not.toContain("(declare-fun ");
  });

  it("declares uninterpreted predicates as Bool-valued functions", () => {
    const x: IrTerm = { kind: "var", name: "_x0", sort: Int };
    const assertion: IrFormula = {
      kind: "forall",
      sort: Int,
      predicate: {
        kind: "lambda",
        varName: "_x0",
        sort: Int,
        body: { kind: "atomic", predicate: "isPrime", args: [x] },
      },
    };
    const out = emitSmtLibProblem({ axioms: [], assertion });
    expect(out).toContain("(declare-fun isPrime (Int) Bool)");
    expect(out).toContain("(forall ((_x0 Int)) (isPrime _x0))");
  });

  it("declares member/subset as uninterpreted Bool predicates (not SMT-LIB base names)", () => {
    // forSome injects a `member` atomic predicate. Verify the emitted
    // problem declares it rather than referencing an undefined symbol.
    const setVar: IrTerm = { kind: "var", name: "S", sort: SetOf(Int) };
    // Wrap in an exists so the var binder is in scope for the body's
    // member call. We use a hand-rolled IR to keep the set var free.
    const body = forSome(setVar, Int, (x) => A.equal(x, 0));
    const out = emitSmtLibProblem({
      axioms: [],
      assertion: {
        kind: "exists",
        sort: SetOf(Int),
        predicate: { kind: "lambda", varName: "S", sort: SetOf(Int), body },
      },
    });
    expect(out).toContain("(declare-fun member (Int (Set Int)) Bool)");
    expect(out).toContain("(member ");
  });

  it("respects custom logic argument", () => {
    const out = emitSmtLibProblem({
      axioms: [],
      assertion: A.equal(1, 1),
      logic: "QF_LIA",
    });
    expect(out.startsWith("(set-logic QF_LIA)")).toBe(true);
  });

  it("orders axioms before the negated assertion", () => {
    const ax = A.equal(1, 1);
    const claim = A.equal(2, 2);
    const out = emitSmtLibProblem({ axioms: [ax], assertion: claim });
    const axIdx = out.indexOf("(assert (= 1 1))");
    const negIdx = out.indexOf("(assert (not (= 2 2)))");
    expect(axIdx).toBeGreaterThan(0);
    expect(negIdx).toBeGreaterThan(axIdx);
  });
});

// ---------------------------------------------------------------------------
// Round-trip structural sanity
// ---------------------------------------------------------------------------

describe("emitSmtLib — structural sanity", () => {
  it("output has balanced parentheses", () => {
    const f = forAll(Int, (x) =>
      exists(Int, (y) => and(A.lessThan(x, y), implies(A.equal(x, 0), A.equal(y, 1)))),
    );
    const s = emitSmtLib(f);
    let depth = 0;
    for (const ch of s) {
      if (ch === "(") depth++;
      else if (ch === ")") depth--;
      expect(depth).toBeGreaterThanOrEqual(0);
    }
    expect(depth).toBe(0);
  });

  it("full problem ends with check-sat", () => {
    const out = emitSmtLibProblem({ axioms: [], assertion: A.equal(1, 1) });
    expect(out.trimEnd().endsWith("(check-sat)")).toBe(true);
  });
});
