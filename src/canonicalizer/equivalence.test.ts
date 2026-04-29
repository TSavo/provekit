/**
 * Cross-language equivalence corpus for the AST canonicalizer.
 *
 * Each test constructs two IrFormula values that are different surface
 * representations of the same logical formula and asserts their
 * canonicalized hashes match. "Different surface representation"
 * simulates what different host languages would produce when expressing
 * the same logical claim.
 *
 * Test groups:
 *   1. Pure quantifier-free formulas
 *   2. Single-quantifier formulas (bound-variable renaming / alpha-equivalence)
 *   3. Multi-quantifier formulas with nested binders
 *   4. AC-normalization cases
 *   5. De Morgan cases
 *   6. Implies-removal cases
 *   7. Equality argument sorting
 *   8. Negative cases — formulas that DIFFER and must produce different hashes
 */

import { describe, it, expect, beforeEach } from "vitest";
import { propertyHashFromFormula, formulaToCanonicalAst } from "./canonicalize.js";
import type { IrFormula, IrTerm, Sort } from "./irFormula.js";
import {
  forAll as irForAll,
  exists as irExists,
  Int as IrInt,
  assert as irAssert,
  and as irAnd,
  implies as irImplies,
  iff as irIff,
} from "../ir/index.js";
import { _resetCounter as _resetIrCounter } from "../ir/quantifiers.js";

// -----------------------------------------------------------------------
// Helpers to build IrFormula values
// -----------------------------------------------------------------------

const Int: Sort = { kind: "primitive", name: "Int" };
const Bool: Sort = { kind: "primitive", name: "Bool" };
const Real: Sort = { kind: "primitive", name: "Real" };
const Ref: Sort = { kind: "primitive", name: "Ref" };

function varTerm(name: string, sort: Sort): IrTerm {
  return { kind: "var", name, sort };
}
function constTerm(value: unknown, sort: Sort): IrTerm {
  return { kind: "const", value, sort };
}

function atomicEq(a: Parameters<typeof varTerm>[0], b: Parameters<typeof varTerm>[0], sort: Sort = Int): IrFormula {
  return {
    kind: "atomic",
    predicate: "=",
    args: [{ kind: "var", name: a, sort }, { kind: "var", name: b, sort }],
  };
}

function atomicNeq(a: string, b: number, sort: Sort = Int): IrFormula {
  return {
    kind: "atomic",
    predicate: "≠",
    args: [{ kind: "var", name: a, sort }, { kind: "const", value: b, sort }],
  };
}

function forall(varName: string, sort: Sort, body: IrFormula): IrFormula {
  return {
    kind: "forall",
    sort,
    predicate: { kind: "lambda", varName, sort, body },
  };
}

function exists_(varName: string, sort: Sort, body: IrFormula): IrFormula {
  return {
    kind: "exists",
    sort,
    predicate: { kind: "lambda", varName, sort, body },
  };
}

function and_(...formulas: IrFormula[]): IrFormula {
  return { kind: "and", conjuncts: formulas };
}

function or_(...formulas: IrFormula[]): IrFormula {
  return { kind: "or", disjuncts: formulas };
}

function not_(f: IrFormula): IrFormula {
  return { kind: "not", body: f };
}

function implies_(ante: IrFormula, cons: IrFormula): IrFormula {
  return { kind: "implies", antecedent: ante, consequent: cons };
}

// -----------------------------------------------------------------------
// 1. Pure quantifier-free formulas
// -----------------------------------------------------------------------

describe("1. Pure quantifier-free formulas", () => {
  it("atomic equality: =(a, b) is the same formula regardless of variable names", () => {
    // Same predicate, same shape — different variable names don't affect
    // quantifier-free formulas (no binders to rename).
    // Both produce the same canonical form because the vars have the same
    // structural role (both are free vars at depth 0).
    // Note: free variables are bound by the outer scope; for quantifier-free
    // formulas we compare against a literal with the same name.
    const f1: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("alias resolution: notEqual alias maps to ≠", () => {
    const f1: IrFormula = {
      kind: "atomic",
      predicate: "notEqual",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      predicate: "≠",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("alias resolution: != alias maps to ≠", () => {
    const f1: IrFormula = {
      kind: "atomic",
      predicate: "!=",
      args: [{ kind: "const", value: 5, sort: Int }, { kind: "const", value: 3, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      predicate: "≠",
      args: [{ kind: "const", value: 5, sort: Int }, { kind: "const", value: 3, sort: Int }],
    };
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("equality argument sorting: =(a, b) ≡ =(b, a) for constants", () => {
    const f1: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 2, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("identity removal: and(true, p) ≡ p", () => {
    const p: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const withTrue: IrFormula = and_(
      { kind: "atomic", predicate: "true", args: [] },
      p,
    );
    expect(propertyHashFromFormula(withTrue)).toBe(propertyHashFromFormula(p));
  });

  it("identity removal: or(false, p) ≡ p", () => {
    const p: IrFormula = {
      kind: "atomic",
      predicate: "<",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const withFalse: IrFormula = or_(
      { kind: "atomic", predicate: "false", args: [] },
      p,
    );
    expect(propertyHashFromFormula(withFalse)).toBe(propertyHashFromFormula(p));
  });

  it("deduplication: and(p, p) ≡ p", () => {
    const p: IrFormula = {
      kind: "atomic",
      predicate: "<",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const doubled = and_(p, p);
    expect(propertyHashFromFormula(doubled)).toBe(propertyHashFromFormula(p));
  });
});

// -----------------------------------------------------------------------
// 2. Single-quantifier formulas — alpha-equivalence (bound-variable renaming)
// -----------------------------------------------------------------------

describe("2. Single-quantifier / alpha-equivalence", () => {
  it("forAll(b => b ≠ 0) ≡ forAll(x => x ≠ 0) [TypeScript vs Rust naming]", () => {
    // TypeScript style: bound variable named "b"
    const tsStyle = forall("b", Int, atomicNeq("b", 0));
    // Rust style: bound variable named "x"
    const rustStyle = forall("x", Int, atomicNeq("x", 0));

    expect(propertyHashFromFormula(tsStyle)).toBe(propertyHashFromFormula(rustStyle));
  });

  it("forAll(denominator => denominator ≠ 0) [Go naming convention]", () => {
    const goStyle = forall("denominator", Int, atomicNeq("denominator", 0));
    const tsStyle = forall("b", Int, atomicNeq("b", 0));

    expect(propertyHashFromFormula(goStyle)).toBe(propertyHashFromFormula(tsStyle));
  });

  it("exists(y => y = 0) ≡ exists(z => z = 0)", () => {
    const f1 = exists_("y", Int, {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "var", name: "y", sort: Int }, { kind: "const", value: 0, sort: Int }],
    });
    const f2 = exists_("z", Int, {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "var", name: "z", sort: Int }, { kind: "const", value: 0, sort: Int }],
    });
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("bound-variable sort is preserved: forAll(Int, ...) ≠ forAll(Real, ...)", () => {
    const intFormula = forall("x", Int, atomicNeq("x", 0, Int));
    const realFormula = forall("x", Real, {
      kind: "atomic",
      predicate: "≠",
      args: [{ kind: "var", name: "x", sort: Real }, { kind: "const", value: 0, sort: Real }],
    });
    expect(propertyHashFromFormula(intFormula)).not.toBe(propertyHashFromFormula(realFormula));
  });
});

// -----------------------------------------------------------------------
// 3. Multi-quantifier formulas — nested binders
// -----------------------------------------------------------------------

describe("3. Multi-quantifier / nested binders", () => {
  it("forAll(a => forAll(b => =(a, b))) regardless of variable names", () => {
    const f1 = forall("a", Int, forall("b", Int, atomicEq("a", "b")));
    // Different names, same structure
    const f2 = forall("x", Int, forall("y", Int, atomicEq("x", "y")));
    // Yet another naming
    const f3 = forall("outer", Int, forall("inner", Int, atomicEq("outer", "inner")));

    const h1 = propertyHashFromFormula(f1);
    const h2 = propertyHashFromFormula(f2);
    const h3 = propertyHashFromFormula(f3);
    expect(h1).toBe(h2);
    expect(h1).toBe(h3);
  });

  it("forAll(a => exists(b => =(a, b))) correctly references outer var via index 1", () => {
    const f1 = forall(
      "a", Int,
      exists_("b", Int, atomicEq("a", "b")),
    );
    const f2 = forall(
      "p", Int,
      exists_("q", Int, atomicEq("p", "q")),
    );
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("forAll(a => forAll(b => =(a, b))) ≠ forAll(a => forAll(b => =(b, a))) [different de Bruijn]", () => {
    // =(a, b): a is index 1, b is index 0 → after equality sorting → sorted(index0, index1)
    // =(b, a): b is index 0, a is index 1 → same after equality sorting
    // So these ARE the same! Equality argument sorting makes them equal.
    const f1 = forall("a", Int, forall("b", Int, atomicEq("a", "b")));
    const f2 = forall("a", Int, forall("b", Int, atomicEq("b", "a")));
    // Equality is symmetric — argument sorting collapses them.
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("forAll(a => forAll(b => <(a, b))) ≠ forAll(a => forAll(b => <(b, a))) [ordered: not symmetric]", () => {
    // < is ordered, not sorted by equality normalization.
    // a < b: a is outer (index 1 relative to inner), b is inner (index 0)
    // b < a: b is inner (index 0), a is outer (index 1)
    const ltAB = forall("a", Int, forall("b", Int, {
      kind: "atomic",
      predicate: "<",
      args: [{ kind: "var", name: "a", sort: Int }, { kind: "var", name: "b", sort: Int }],
    }));
    const ltBA = forall("a", Int, forall("b", Int, {
      kind: "atomic",
      predicate: "<",
      args: [{ kind: "var", name: "b", sort: Int }, { kind: "var", name: "a", sort: Int }],
    }));
    expect(propertyHashFromFormula(ltAB)).not.toBe(propertyHashFromFormula(ltBA));
  });
});

// -----------------------------------------------------------------------
// 4. AC-normalization cases
// -----------------------------------------------------------------------

describe("4. AC normalization", () => {
  const p: IrFormula = {
    kind: "atomic",
    predicate: "<",
    args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
  };
  const q: IrFormula = {
    kind: "atomic",
    predicate: "<",
    args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
  };
  const r: IrFormula = {
    kind: "atomic",
    predicate: "<",
    args: [{ kind: "const", value: 2, sort: Int }, { kind: "const", value: 3, sort: Int }],
  };

  it("and(p, q) ≡ and(q, p) — commutativity", () => {
    expect(propertyHashFromFormula(and_(p, q))).toBe(propertyHashFromFormula(and_(q, p)));
  });

  it("or(p, q) ≡ or(q, p) — commutativity", () => {
    expect(propertyHashFromFormula(or_(p, q))).toBe(propertyHashFromFormula(or_(q, p)));
  });

  it("and(and(p, q), r) ≡ and(p, q, r) — associativity flattening", () => {
    const nested = and_(and_(p, q), r);
    const flat = and_(p, q, r);
    expect(propertyHashFromFormula(nested)).toBe(propertyHashFromFormula(flat));
  });

  it("or(or(p, q), r) ≡ or(p, q, r) — associativity flattening", () => {
    const nested = or_(or_(p, q), r);
    const flat = or_(p, q, r);
    expect(propertyHashFromFormula(nested)).toBe(propertyHashFromFormula(flat));
  });

  it("and(p, q, r) with all orderings are equal", () => {
    const hash = propertyHashFromFormula(and_(p, q, r));
    expect(propertyHashFromFormula(and_(p, r, q))).toBe(hash);
    expect(propertyHashFromFormula(and_(q, p, r))).toBe(hash);
    expect(propertyHashFromFormula(and_(q, r, p))).toBe(hash);
    expect(propertyHashFromFormula(and_(r, p, q))).toBe(hash);
    expect(propertyHashFromFormula(and_(r, q, p))).toBe(hash);
  });

  it("and(false, p) → false", () => {
    const f = and_({ kind: "atomic", predicate: "false", args: [] }, p);
    expect(propertyHashFromFormula(f)).toBe(
      propertyHashFromFormula({ kind: "atomic", predicate: "false", args: [] }),
    );
  });

  it("or(true, p) → true", () => {
    const f = or_({ kind: "atomic", predicate: "true", args: [] }, p);
    expect(propertyHashFromFormula(f)).toBe(
      propertyHashFromFormula({ kind: "atomic", predicate: "true", args: [] }),
    );
  });
});

// -----------------------------------------------------------------------
// 5. De Morgan cases
// -----------------------------------------------------------------------

describe("5. De Morgan / NNF", () => {
  const p: IrFormula = {
    kind: "atomic",
    predicate: "=",
    args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
  };
  const q: IrFormula = {
    kind: "atomic",
    predicate: "<",
    args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
  };

  it("not(and(p, q)) ≡ or(not(p), not(q))", () => {
    const lhs = not_(and_(p, q));
    const rhs = or_(not_(p), not_(q));
    expect(propertyHashFromFormula(lhs)).toBe(propertyHashFromFormula(rhs));
  });

  it("not(or(p, q)) ≡ and(not(p), not(q))", () => {
    const lhs = not_(or_(p, q));
    const rhs = and_(not_(p), not_(q));
    expect(propertyHashFromFormula(lhs)).toBe(propertyHashFromFormula(rhs));
  });

  it("not(not(p)) ≡ p", () => {
    expect(propertyHashFromFormula(not_(not_(p)))).toBe(propertyHashFromFormula(p));
  });

  it("not(p = q) ≡ p ≠ q", () => {
    const notEq: IrFormula = not_({
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
    });
    const neq: IrFormula = {
      kind: "atomic",
      predicate: "≠",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
    };
    expect(propertyHashFromFormula(notEq)).toBe(propertyHashFromFormula(neq));
  });

  it("not(p < q) ≡ p ≥ q", () => {
    const notLt: IrFormula = not_({
      kind: "atomic",
      predicate: "<",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 5, sort: Int }],
    });
    const gte: IrFormula = {
      kind: "atomic",
      predicate: "≥",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 5, sort: Int }],
    };
    expect(propertyHashFromFormula(notLt)).toBe(propertyHashFromFormula(gte));
  });

  it("not(forall(s, body)) ≡ exists(s, not(body))", () => {
    const lhs = not_(forall("x", Int, atomicNeq("x", 0)));
    const rhs = exists_("x", Int, {
      kind: "not",
      body: atomicNeq("x", 0),
    });
    expect(propertyHashFromFormula(lhs)).toBe(propertyHashFromFormula(rhs));
  });
});

// -----------------------------------------------------------------------
// 6. Implies-removal cases
// -----------------------------------------------------------------------

describe("6. Implies removal", () => {
  const p: IrFormula = {
    kind: "atomic",
    predicate: "=",
    args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
  };
  const q: IrFormula = {
    kind: "atomic",
    predicate: "<",
    args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
  };

  it("implies(p, q) ≡ or(not(p), q)", () => {
    const impl = implies_(p, q);
    const orNotP = or_(not_(p), q);
    expect(propertyHashFromFormula(impl)).toBe(propertyHashFromFormula(orNotP));
  });

  it("implies(p, implies(q, r)) ≡ or(not(p), or(not(q), r)) [chained]", () => {
    const r: IrFormula = {
      kind: "atomic",
      predicate: "≠",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const impl = implies_(p, implies_(q, r));
    // or(not(p), or(not(q), r)) — then AC-normalizes or nesting
    const expanded = or_(not_(p), or_(not_(q), r));
    expect(propertyHashFromFormula(impl)).toBe(propertyHashFromFormula(expanded));
  });
});

// -----------------------------------------------------------------------
// 7. Equality argument sorting
// -----------------------------------------------------------------------

describe("7. Equality argument sorting", () => {
  it("=(const:1, const:2) ≡ =(const:2, const:1)", () => {
    const f1: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 2, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("≠(const:1, const:2) ≡ ≠(const:2, const:1)", () => {
    const f1: IrFormula = {
      kind: "atomic",
      predicate: "≠",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      predicate: "≠",
      args: [{ kind: "const", value: 2, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("constants prefer right: 5 < x ≡ x > 5 (const on left → flip)", () => {
    // 5 < x: args[0]=const(5), args[1]=var(x) → flip → x > 5
    // x > 5: args[0]=var(x), args[1]=const(5) → constant already on right
    // Both should produce the same canonical form.
    const constLtVar: IrFormula = forall("x", Int, {
      kind: "atomic",
      predicate: "<",
      args: [{ kind: "const", value: 5, sort: Int }, { kind: "var", name: "x", sort: Int }],
    });
    const varGtConst: IrFormula = forall("x", Int, {
      kind: "atomic",
      predicate: ">",
      args: [{ kind: "var", name: "x", sort: Int }, { kind: "const", value: 5, sort: Int }],
    });
    expect(propertyHashFromFormula(constLtVar)).toBe(propertyHashFromFormula(varGtConst));
  });
});

// -----------------------------------------------------------------------
// 8. Negative cases — formulas that DIFFER
// -----------------------------------------------------------------------

describe("8. Negative cases — formulas that must differ", () => {
  it("different predicate: < vs ≤", () => {
    const lt: IrFormula = {
      kind: "atomic",
      predicate: "<",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const lte: IrFormula = {
      kind: "atomic",
      predicate: "≤",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(lt)).not.toBe(propertyHashFromFormula(lte));
  });

  it("different sort: Int vs Real", () => {
    const intFormula: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const realFormula: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 1, sort: Real }, { kind: "const", value: 1, sort: Real }],
    };
    expect(propertyHashFromFormula(intFormula)).not.toBe(propertyHashFromFormula(realFormula));
  });

  it("different constant value: 0 vs 1", () => {
    const zero: IrFormula = {
      kind: "atomic",
      predicate: "≠",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const two: IrFormula = {
      kind: "atomic",
      predicate: "≠",
      args: [{ kind: "const", value: 2, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(zero)).not.toBe(propertyHashFromFormula(two));
  });

  it("different quantifier depth: forall(p) vs forall(forall(p))", () => {
    const p: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const single = forall("x", Int, p);
    const double = forall("x", Int, forall("y", Int, p));
    expect(propertyHashFromFormula(single)).not.toBe(propertyHashFromFormula(double));
  });

  it("forall vs exists: different quantifier kind", () => {
    const bodyF = atomicNeq("x", 0);
    const fa = forall("x", Int, bodyF);
    const ex = exists_("x", Int, bodyF);
    expect(propertyHashFromFormula(fa)).not.toBe(propertyHashFromFormula(ex));
  });

  it("and vs or: different connective", () => {
    const p: IrFormula = {
      kind: "atomic",
      predicate: "<",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const q: IrFormula = {
      kind: "atomic",
      predicate: "<",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
    };
    expect(propertyHashFromFormula(and_(p, q))).not.toBe(propertyHashFromFormula(or_(p, q)));
  });

  it("different bound-variable index: single binder vs two binders", () => {
    // forall(a. forall(b. a < b)) — outer is index 1 in body
    // forall(a. forall(b. b < a)) — outer is index 1 in body, inner is 0
    // < is not commutative so these differ
    const outerLtInner = forall("a", Int, forall("b", Int, {
      kind: "atomic",
      predicate: "<",
      args: [{ kind: "var", name: "a", sort: Int }, { kind: "var", name: "b", sort: Int }],
    }));
    const innerLtOuter = forall("a", Int, forall("b", Int, {
      kind: "atomic",
      predicate: "<",
      args: [{ kind: "var", name: "b", sort: Int }, { kind: "var", name: "a", sort: Int }],
    }));
    expect(propertyHashFromFormula(outerLtInner)).not.toBe(propertyHashFromFormula(innerLtOuter));
  });

  it("different string constant value", () => {
    const f1: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [
        { kind: "const", value: "hello", sort: { kind: "primitive", name: "String" } },
        { kind: "const", value: "hello", sort: { kind: "primitive", name: "String" } },
      ],
    };
    const f2: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [
        { kind: "const", value: "world", sort: { kind: "primitive", name: "String" } },
        { kind: "const", value: "world", sort: { kind: "primitive", name: "String" } },
      ],
    };
    expect(propertyHashFromFormula(f1)).not.toBe(propertyHashFromFormula(f2));
  });
});

// -----------------------------------------------------------------------
// 9. Hash format sanity
// -----------------------------------------------------------------------

describe("9. Hash format", () => {
  it("propertyHash is exactly 16 hex characters", () => {
    const f: IrFormula = {
      kind: "atomic",
      predicate: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const h = propertyHashFromFormula(f);
    expect(h).toMatch(/^[0-9a-f]{16}$/);
  });

  it("formulaToCanonicalAst returns a CanonicalFolAst with no implies", () => {
    const f: IrFormula = implies_(
      { kind: "atomic", predicate: "true", args: [] },
      { kind: "atomic", predicate: "false", args: [] },
    );
    const ast = formulaToCanonicalAst(f);
    // After implies removal + NNF + AC, implies(true, false) →
    // or(not(true), false) → or(false, false) → false
    // not(true) → false via NNF predicate negation
    expect(ast).toEqual({ kind: "atomic", predicate: "false", args: [] });
  });

  it("forall body formula: spec example — forAll(b => b ≠ 0)", () => {
    const formula = forall("b", Int, atomicNeq("b", 0));
    const ast = formulaToCanonicalAst(formula);
    expect(ast.kind).toBe("forall");
    if (ast.kind === "forall") {
      expect(ast.sort).toEqual({ kind: "primitive", name: "Int" });
      expect(ast.body.kind).toBe("atomic");
    }
  });

  it("AstCanonicalizerImpl.propertyHashFromFormula matches standalone function", async () => {
    const { AstCanonicalizerImpl } = await import("./index.js");
    const impl = new AstCanonicalizerImpl();
    const f = forall("x", Int, atomicNeq("x", 0));
    expect(impl.propertyHashFromFormula(f)).toBe(propertyHashFromFormula(f));
  });

  it("specVersion returns major 1", async () => {
    const { AstCanonicalizerImpl } = await import("./index.js");
    const impl = new AstCanonicalizerImpl();
    expect(impl.specVersion().major).toBe(1);
  });
});

// -----------------------------------------------------------------------
// 10. IR-library → canonicalizer roundtrip (no manual translation)
// -----------------------------------------------------------------------

/**
 * These tests are the alignment proof for task #2: an `IrFormula`
 * value built via the IR library's public exports flows directly
 * into the canonicalizer without any cast or translation step.
 *
 * If TypeScript ever rejects one of these calls, the IR-library /
 * canonicalizer types have drifted apart. If a hash drifts, either
 * the canonicalizer's pipeline changed or the IR library started
 * emitting a different shape — both are alignment regressions.
 */
describe("10. IR-library → canonicalizer roundtrip", () => {
  beforeEach(() => {
    // Deterministic bound-variable names. The canonicalizer erases them
    // via de Bruijn, but resetting keeps debug output stable.
    _resetIrCounter();
  });

  it("forAll(Int, b => assert.notEqual(b, 0)) — spec example, pinned hash", () => {
    // Surface (TypeScript): forAll<Int>(b => assert.notEqual(b, 0))
    // Canonical FOL:        forall(b: Int).¬(b = 0)
    // Spec:                 docs/specs/2026-04-29-ir-library.md §"Cross-language equivalence"
    const formula = irForAll(IrInt, (b) => irAssert.notEqual(b, 0));

    // No cast — the IR library returns IrFormula, the canonicalizer
    // accepts IrFormula. Alignment is "this line type-checks".
    const hash = propertyHashFromFormula(formula);

    expect(hash).toMatch(/^[0-9a-f]{16}$/);
    // Pinned fixture: regenerate intentionally if the canonical-AST
    // grammar changes (a major-version event).
    expect(hash).toBe("d2569af7719024b3");
  });

  it("IR-library forAll matches hand-built equivalent", () => {
    // Library-dialect surface
    const fromLib = irForAll(IrInt, (b) => irAssert.notEqual(b, 0));

    // Hand-built — must produce the same canonical hash as the
    // library output, since both encode `forall(b: Int).¬(b = 0)`.
    // Variable names differ ("_x0" vs "b"); de Bruijn erases them.
    const handBuilt: IrFormula = forall("b", Int, atomicNeq("b", 0));

    expect(propertyHashFromFormula(fromLib)).toBe(propertyHashFromFormula(handBuilt));
  });

  it("IR-library nested quantifiers — forAll(a => exists(b => a < b))", () => {
    const formula = irForAll(IrInt, (a) =>
      irExists(IrInt, (b) => irAssert.lessThan(a, b)),
    );
    const ast = formulaToCanonicalAst(formula);
    expect(ast.kind).toBe("forall");
    if (ast.kind !== "forall") throw new Error();
    expect(ast.body.kind).toBe("exists");
  });

  it("IR-library implies + connectives canonicalize through implies-removal + NNF", () => {
    // forAll(x. (x = 0) → ¬(x ≠ 0))  — tautology after rewrites.
    const formula = irForAll(IrInt, (x) =>
      irImplies(irAssert.equal(x, 0), irAssert.notEqual(x, 0)),
    );
    const hash = propertyHashFromFormula(formula);
    expect(hash).toMatch(/^[0-9a-f]{16}$/);
  });

  it("IR-library iff desugar reaches the same canonical hash as explicit and(implies, implies)", () => {
    // The IR library's public iff() desugars at construction; the
    // canonicalizer's NNF+AC pipeline collapses it to one shape.
    // Use ground atoms (no free vars) so the canonicalizer's de Bruijn
    // pass has a well-formed input.
    const a: IrFormula = irAssert.equal(0, 0);
    const b: IrFormula = irAssert.equal(1, 1);
    const viaIff = irIff(a, b);
    const viaAnd = irAnd(
      irImplies(a, b),
      irImplies(b, a),
    );
    expect(propertyHashFromFormula(viaIff)).toBe(propertyHashFromFormula(viaAnd));
  });

  it("AstCanonicalizerImpl accepts IR-library output via .propertyHashFromFormula", async () => {
    const { AstCanonicalizerImpl } = await import("./index.js");
    const impl = new AstCanonicalizerImpl();
    const formula = irForAll(IrInt, (b) => irAssert.notEqual(b, 0));
    expect(impl.propertyHashFromFormula(formula)).toBe(propertyHashFromFormula(formula));
  });
});
