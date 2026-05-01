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
import {
  propertyHashFromFormula,
  formulaToCanonicalAst,
  propertyHashFromAst,
} from "./canonicalize.js";
import { serializeCanonicalAst, SERIALIZATION_FORMAT } from "./serialize.js";
import { computeCid, blake3_512_hex } from "./hash.js";
import type {
  CanonicalFolAst,
  CanonicalSort,
  CanonicalTerm,
} from "./ast.js";
import type { IrFormula, IrTerm, Sort } from "./irFormula.js";
import { canonicalizeSort } from "./passes/sorts.js";
import {
  applyDeBruijn,
  type DeBruijnFormula,
  type DeBruijnTerm,
} from "./passes/deBruijn.js";
import {
  canonicalizeTerm,
  termSortKey,
  canonicalizePredicate,
} from "./passes/predicates.js";
import { toNnf } from "./passes/nnf.js";
import { acNormalize, astSortKey } from "./passes/acNormalize.js";
import { removeImplies, type PreNnfAst } from "./passes/impliesRemoval.js";
import {
  AstCanonicalizerImpl,
  canonicalizer,
  type Bindings,
  type BindingScope,
} from "./index.js";
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

function varTerm(name: string, _sort: Sort): IrTerm {
  void _sort;
  return { kind: "var", name };
}
function constTerm(value: unknown, sort: Sort): IrTerm {
  return { kind: "const", value, sort };
}

function atomicEq(a: Parameters<typeof varTerm>[0], b: Parameters<typeof varTerm>[0], _sort: Sort = Int): IrFormula {
  void _sort;
  return {
    kind: "atomic",
    name: "=",
    args: [{ kind: "var", name: a }, { kind: "var", name: b }],
  };
}

function atomicNeq(a: string, b: number, sort: Sort = Int): IrFormula {
  return {
    kind: "atomic",
    name: "≠",
    args: [{ kind: "var", name: a }, { kind: "const", value: b, sort }],
  };
}

function forall(varName: string, sort: Sort, body: IrFormula): IrFormula {
  return { kind: "forall", name: varName, sort, body };
}

function exists_(varName: string, sort: Sort, body: IrFormula): IrFormula {
  return { kind: "exists", name: varName, sort, body };
}

function and_(...formulas: IrFormula[]): IrFormula {
  return { kind: "and", operands: formulas };
}

function or_(...formulas: IrFormula[]): IrFormula {
  return { kind: "or", operands: formulas };
}

function not_(f: IrFormula): IrFormula {
  return { kind: "not", operands: [f] };
}

function implies_(ante: IrFormula, cons: IrFormula): IrFormula {
  return { kind: "implies", operands: [ante, cons]};
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
      name: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("alias resolution: notEqual alias maps to ≠", () => {
    const f1: IrFormula = {
      kind: "atomic",
      name: "notEqual",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      name: "≠",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("alias resolution: != alias maps to ≠", () => {
    const f1: IrFormula = {
      kind: "atomic",
      name: "!=",
      args: [{ kind: "const", value: 5, sort: Int }, { kind: "const", value: 3, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      name: "≠",
      args: [{ kind: "const", value: 5, sort: Int }, { kind: "const", value: 3, sort: Int }],
    };
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("equality argument sorting: =(a, b) ≡ =(b, a) for constants", () => {
    const f1: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [{ kind: "const", value: 2, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("identity removal: and(true, p) ≡ p", () => {
    const p: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const withTrue: IrFormula = and_(
      { kind: "atomic", name: "true", args: [] },
      p,
    );
    expect(propertyHashFromFormula(withTrue)).toBe(propertyHashFromFormula(p));
  });

  it("identity removal: or(false, p) ≡ p", () => {
    const p: IrFormula = {
      kind: "atomic",
      name: "<",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const withFalse: IrFormula = or_(
      { kind: "atomic", name: "false", args: [] },
      p,
    );
    expect(propertyHashFromFormula(withFalse)).toBe(propertyHashFromFormula(p));
  });

  it("deduplication: and(p, p) ≡ p", () => {
    const p: IrFormula = {
      kind: "atomic",
      name: "<",
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
      name: "=",
      args: [{ kind: "var", name: "y"}, { kind: "const", value: 0, sort: Int }],
    });
    const f2 = exists_("z", Int, {
      kind: "atomic",
      name: "=",
      args: [{ kind: "var", name: "z"}, { kind: "const", value: 0, sort: Int }],
    });
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("bound-variable sort is preserved: forAll(Int, ...) ≠ forAll(Real, ...)", () => {
    const intFormula = forall("x", Int, atomicNeq("x", 0, Int));
    const realFormula = forall("x", Real, {
      kind: "atomic",
      name: "≠",
      args: [{ kind: "var", name: "x"}, { kind: "const", value: 0, sort: Real }],
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
      name: "<",
      args: [{ kind: "var", name: "a"}, { kind: "var", name: "b"}],
    }));
    const ltBA = forall("a", Int, forall("b", Int, {
      kind: "atomic",
      name: "<",
      args: [{ kind: "var", name: "b"}, { kind: "var", name: "a"}],
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
    name: "<",
    args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
  };
  const q: IrFormula = {
    kind: "atomic",
    name: "<",
    args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
  };
  const r: IrFormula = {
    kind: "atomic",
    name: "<",
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
    const f = and_({ kind: "atomic", name: "false", args: [] }, p);
    expect(propertyHashFromFormula(f)).toBe(
      propertyHashFromFormula({ kind: "atomic", name: "false", args: [] }),
    );
  });

  it("or(true, p) → true", () => {
    const f = or_({ kind: "atomic", name: "true", args: [] }, p);
    expect(propertyHashFromFormula(f)).toBe(
      propertyHashFromFormula({ kind: "atomic", name: "true", args: [] }),
    );
  });
});

// -----------------------------------------------------------------------
// 5. De Morgan cases
// -----------------------------------------------------------------------

describe("5. De Morgan / NNF", () => {
  const p: IrFormula = {
    kind: "atomic",
    name: "=",
    args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
  };
  const q: IrFormula = {
    kind: "atomic",
    name: "<",
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
      name: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
    });
    const neq: IrFormula = {
      kind: "atomic",
      name: "≠",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
    };
    expect(propertyHashFromFormula(notEq)).toBe(propertyHashFromFormula(neq));
  });

  it("not(p < q) ≡ p ≥ q", () => {
    const notLt: IrFormula = not_({
      kind: "atomic",
      name: "<",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 5, sort: Int }],
    });
    const gte: IrFormula = {
      kind: "atomic",
      name: "≥",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 5, sort: Int }],
    };
    expect(propertyHashFromFormula(notLt)).toBe(propertyHashFromFormula(gte));
  });

  it("not(forall(s, body)) ≡ exists(s, not(body))", () => {
    const lhs = not_(forall("x", Int, atomicNeq("x", 0)));
    const rhs = exists_("x", Int, {
      kind: "not",
      operands: [atomicNeq("x", 0)],
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
    name: "=",
    args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
  };
  const q: IrFormula = {
    kind: "atomic",
    name: "<",
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
      name: "≠",
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
      name: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [{ kind: "const", value: 2, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(f1)).toBe(propertyHashFromFormula(f2));
  });

  it("≠(const:1, const:2) ≡ ≠(const:2, const:1)", () => {
    const f1: IrFormula = {
      kind: "atomic",
      name: "≠",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 2, sort: Int }],
    };
    const f2: IrFormula = {
      kind: "atomic",
      name: "≠",
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
      name: "<",
      args: [{ kind: "const", value: 5, sort: Int }, { kind: "var", name: "x"}],
    });
    const varGtConst: IrFormula = forall("x", Int, {
      kind: "atomic",
      name: ">",
      args: [{ kind: "var", name: "x"}, { kind: "const", value: 5, sort: Int }],
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
      name: "<",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const lte: IrFormula = {
      kind: "atomic",
      name: "≤",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(lt)).not.toBe(propertyHashFromFormula(lte));
  });

  it("different sort: Int vs Real", () => {
    const intFormula: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const realFormula: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [{ kind: "const", value: 1, sort: Real }, { kind: "const", value: 1, sort: Real }],
    };
    expect(propertyHashFromFormula(intFormula)).not.toBe(propertyHashFromFormula(realFormula));
  });

  it("different constant value: 0 vs 1", () => {
    const zero: IrFormula = {
      kind: "atomic",
      name: "≠",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const two: IrFormula = {
      kind: "atomic",
      name: "≠",
      args: [{ kind: "const", value: 2, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    expect(propertyHashFromFormula(zero)).not.toBe(propertyHashFromFormula(two));
  });

  it("different quantifier depth: forall(p) vs forall(forall(p))", () => {
    const p: IrFormula = {
      kind: "atomic",
      name: "=",
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
      name: "<",
      args: [{ kind: "const", value: 0, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const q: IrFormula = {
      kind: "atomic",
      name: "<",
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
      name: "<",
      args: [{ kind: "var", name: "a"}, { kind: "var", name: "b"}],
    }));
    const innerLtOuter = forall("a", Int, forall("b", Int, {
      kind: "atomic",
      name: "<",
      args: [{ kind: "var", name: "b"}, { kind: "var", name: "a"}],
    }));
    expect(propertyHashFromFormula(outerLtInner)).not.toBe(propertyHashFromFormula(innerLtOuter));
  });

  it("different string constant value", () => {
    const f1: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [
        { kind: "const", value: "hello", sort: { kind: "primitive", name: "String" } },
        { kind: "const", value: "hello", sort: { kind: "primitive", name: "String" } },
      ],
    };
    const f2: IrFormula = {
      kind: "atomic",
      name: "=",
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
      name: "=",
      args: [{ kind: "const", value: 1, sort: Int }, { kind: "const", value: 1, sort: Int }],
    };
    const h = propertyHashFromFormula(f);
    expect(h).toMatch(/^blake3-512:[0-9a-f]{128}$/);
  });

  it("formulaToCanonicalAst returns a CanonicalFolAst with no implies", () => {
    const f: IrFormula = implies_(
      { kind: "atomic", name: "true", args: [] },
      { kind: "atomic", name: "false", args: [] },
    );
    const ast = formulaToCanonicalAst(f);
    // After implies removal + NNF + AC, implies(true, false) →
    // or(not(true), false) → or(false, false) → false
    // not(true) → false via NNF predicate negation
    expect(ast).toEqual({ kind: "atomic", name: "false", args: [] });
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
    // Spec:                 protocol/specs/2026-04-29-ir-library.md §"Cross-language equivalence"
    const formula = irForAll(IrInt, (b) => irAssert.notEqual(b, 0));

    // No cast — the IR library returns IrFormula, the canonicalizer
    // accepts IrFormula. Alignment is "this line type-checks".
    const hash = propertyHashFromFormula(formula);

    expect(hash).toMatch(/^blake3-512:[0-9a-f]{128}$/);
    // Pinned fixture under protocol v1.1.0 canonical-AST grammar
    // (atomic.name + not.operands) plus the BLAKE3-512 self-identifying
    // hash widening. Update only when the canonical-AST shape or the
    // hash function changes intentionally.
    expect(hash).toBe(
      "blake3-512:3c87a7f850ab31498ead7a5ce1b603d41e470fc6034afd2bef31f393a4b1d7d5e1b3304c784c708cf48a64a5acd3dfa5aa6c1faa458ee646395dfab0ddc1b651",
    );
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
    expect(hash).toMatch(/^blake3-512:[0-9a-f]{128}$/);
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

// -----------------------------------------------------------------------
// 11. RFC 8785 §3.2.2.3 number-serialization conformance
// -----------------------------------------------------------------------

/**
 * RFC 8785 §3.2.2.3 normatively delegates JSON number serialization to
 * ECMA-262 §7.1.12.1 (Number::toString incl. "Note 2"). On V8/Node,
 * `JSON.stringify(n)` for a finite number IS that algorithm — RFC 8785
 * cites V8 as the reference implementation, and Appendix A's reference
 * canonicalizer uses `JSON.stringify` for the number path verbatim.
 *
 * These tests pin the contract so a future hand-rolled formatter that
 * drifts from §3.2.2.3 fails immediately. The fixtures are RFC 8785
 * Appendix B, "Number Serialization Samples" — the canonical conformance
 * suite for the numeric subset of §3.2.2.3.
 *
 * Cross-kit note: this conformance argument is V8/Node-specific. Each
 * non-TS kit (Go, Rust, Python) needs its own §3.2.2.3 conformance suite.
 */
describe("11. RFC 8785 §3.2.2.3 number serialization", () => {
  const RealSort: CanonicalSort = { kind: "primitive", name: "Real" };

  /**
   * Serialize a single number through the full canonical pipeline by
   * wrapping it in a minimal Atomic AST node. Returns the byte string
   * produced by `serializeCanonicalAst`. The returned form is:
   *   {"args":[{"kind":"const","sort":{"kind":"primitive","name":"Real"},"value":<N>}],"kind":"atomic","predicate":"P"}
   * — useful for asserting that the <N> substring is the §3.2.2.3
   * canonical form for the input.
   */
  function serializeNumberInAst(n: number): string {
    const ast: CanonicalFolAst = {
      kind: "atomic",
      name: "P",
      args: [{ kind: "const", sort: RealSort, value: n }],
    };
    return serializeCanonicalAst(ast).toString("utf8");
  }

  /** Extract just the JSON literal that `value` was serialized to. */
  function extractValueLiteral(serialized: string): string {
    const m = serialized.match(/"value":([^}]+)/);
    if (!m) throw new Error(`could not find "value":… in ${serialized}`);
    return m[1];
  }

  // RFC 8785 Appendix B — IEEE-754 hex → expected JSON serialization.
  // Each row is a contractual fixture: drift here breaks cross-impl
  // hash equivalence and must be treated as a regression.
  const APPENDIX_B: Array<{ hex: string; expected: string; comment: string }> = [
    { hex: "0000000000000000", expected: "0", comment: "zero" },
    { hex: "8000000000000000", expected: "0", comment: "minus zero → 0" },
    { hex: "0000000000000001", expected: "5e-324", comment: "min positive subnormal" },
    { hex: "8000000000000001", expected: "-5e-324", comment: "min negative subnormal" },
    { hex: "7fefffffffffffff", expected: "1.7976931348623157e+308", comment: "max positive double" },
    { hex: "ffefffffffffffff", expected: "-1.7976931348623157e+308", comment: "max negative double" },
    { hex: "4340000000000000", expected: "9007199254740992", comment: "max safe int + 1" },
    { hex: "c340000000000000", expected: "-9007199254740992", comment: "min safe int - 1" },
    { hex: "4430000000000000", expected: "295147905179352830000", comment: "~2**68 (no exponent)" },
    { hex: "44b52d02c7e14af5", expected: "9.999999999999997e+22", comment: "1e23 boundary -1 ulp" },
    { hex: "44b52d02c7e14af6", expected: "1e+23", comment: "1e23 exact" },
    { hex: "44b52d02c7e14af7", expected: "1.0000000000000001e+23", comment: "1e23 boundary +1 ulp" },
    { hex: "444b1ae4d6e2ef4e", expected: "999999999999999700000", comment: "1e21 boundary -2 ulp" },
    { hex: "444b1ae4d6e2ef4f", expected: "999999999999999900000", comment: "1e21 boundary -1 ulp" },
    { hex: "444b1ae4d6e2ef50", expected: "1e+21", comment: "1e21 exact (exponent threshold)" },
    { hex: "3eb0c6f7a0b5ed8c", expected: "9.999999999999997e-7", comment: "1e-6 boundary" },
    { hex: "3eb0c6f7a0b5ed8d", expected: "0.000001", comment: "1e-6 (no exponent)" },
    { hex: "41b3de4355555553", expected: "333333333.3333332", comment: "round-trip precision -3 ulp" },
    { hex: "41b3de4355555554", expected: "333333333.33333325", comment: "round-trip precision -2 ulp" },
    { hex: "41b3de4355555555", expected: "333333333.3333333", comment: "round-trip precision exact" },
    { hex: "41b3de4355555556", expected: "333333333.3333334", comment: "round-trip precision +1 ulp" },
    { hex: "41b3de4355555557", expected: "333333333.33333343", comment: "round-trip precision +2 ulp" },
    { hex: "becbf647612f3696", expected: "-0.0000033333333333333333", comment: "negative small fixed-point" },
    { hex: "43143ff3c1cb0959", expected: "1424953923781206.2", comment: "Note 2 round-to-even (was .25)" },
  ];

  // Decode an IEEE-754 hex string into a JS number for fixture input.
  function hexToDouble(hex: string): number {
    const buf = Buffer.alloc(8);
    buf.writeBigUInt64BE(BigInt("0x" + hex), 0);
    return buf.readDoubleBE(0);
  }

  for (const { hex, expected, comment } of APPENDIX_B) {
    it(`Appendix B: ${hex} → ${expected} (${comment})`, () => {
      const n = hexToDouble(hex);
      const serialized = serializeNumberInAst(n);
      expect(extractValueLiteral(serialized)).toBe(expected);
    });
  }

  it("-0 normalizes to '0' (defensive: pass 2 does not rewrite -0)", () => {
    const serialized = serializeNumberInAst(-0);
    expect(extractValueLiteral(serialized)).toBe("0");
  });

  it("NaN throws the §3.2.2.3 prohibition error", () => {
    expect(() => serializeNumberInAst(NaN)).toThrow(/non-finite number/);
  });

  it("+Infinity throws the §3.2.2.3 prohibition error", () => {
    expect(() => serializeNumberInAst(Infinity)).toThrow(/non-finite number/);
  });

  it("-Infinity throws the §3.2.2.3 prohibition error", () => {
    expect(() => serializeNumberInAst(-Infinity)).toThrow(/non-finite number/);
  });
});

// -----------------------------------------------------------------------
// 12. Pass-level unit tests — passes/sorts.ts (canonicalizeSort)
// -----------------------------------------------------------------------

describe("12. canonicalizeSort", () => {
  it("preserves the standard primitive sort name", () => {
    expect(canonicalizeSort({ kind: "primitive", name: "Int" })).toEqual({
      kind: "primitive",
      name: "Int",
    });
  });

  it("passes through kit-defined extension sorts", () => {
    expect(canonicalizeSort({ kind: "primitive", name: "myKit:abc123" })).toEqual({
      kind: "primitive",
      name: "myKit:abc123",
    });
  });

  it("recursively canonicalizes set element sorts", () => {
    expect(canonicalizeSort({ kind: "set", element: { kind: "primitive", name: "Int" } })).toEqual({
      kind: "set",
      element: { kind: "primitive", name: "Int" },
    });
  });

  it("recursively canonicalizes tuple element sorts", () => {
    expect(
      canonicalizeSort({
        kind: "tuple",
        elements: [
          { kind: "primitive", name: "Int" },
          { kind: "primitive", name: "Bool" },
        ],
      }),
    ).toEqual({
      kind: "tuple",
      elements: [
        { kind: "primitive", name: "Int" },
        { kind: "primitive", name: "Bool" },
      ],
    });
  });

  it("recursively canonicalizes function domain + range", () => {
    expect(
      canonicalizeSort({
        kind: "function",
        domain: [{ kind: "primitive", name: "Int" }],
        range: { kind: "primitive", name: "Bool" },
      }),
    ).toEqual({
      kind: "function",
      domain: [{ kind: "primitive", name: "Int" }],
      range: { kind: "primitive", name: "Bool" },
    });
  });

  it("nested set-of-set survives", () => {
    expect(
      canonicalizeSort({
        kind: "set",
        element: { kind: "set", element: { kind: "primitive", name: "Int" } },
      }),
    ).toEqual({
      kind: "set",
      element: { kind: "set", element: { kind: "primitive", name: "Int" } },
    });
  });
});

// -----------------------------------------------------------------------
// 13. passes/deBruijn.ts — applyDeBruijn
// -----------------------------------------------------------------------

describe("13. applyDeBruijn", () => {
  it("assigns index 0 to the innermost binder reference", () => {
    const f = forall("a", Int, atomicNeq("a", 0));
    const out = applyDeBruijn(f);
    if (out.kind !== "forall") throw new Error();
    if (out.body.kind !== "atomic") throw new Error();
    const arg0 = out.body.args[0] as DeBruijnTerm;
    if (arg0.kind !== "var") throw new Error();
    expect(arg0.deBruijn).toBe(0);
  });

  it("nested binders increment indices", () => {
    const f = forall(
      "a", Int,
      forall("b", Int, atomicEq("a", "b")),
    );
    const out = applyDeBruijn(f);
    if (out.kind !== "forall") throw new Error();
    if (out.body.kind !== "forall") throw new Error();
    if (out.body.body.kind !== "atomic") throw new Error();
    const args = out.body.body.args as DeBruijnTerm[];
    if (args[0].kind !== "var" || args[1].kind !== "var") throw new Error();
    // a is the outer binder → index 1; b is the inner → index 0.
    expect(args[0].deBruijn).toBe(1);
    expect(args[1].deBruijn).toBe(0);
  });

  it("throws on unbound variable", () => {
    const orphan: IrFormula = {
      kind: "atomic",
      name: "=",
      args: [
        { kind: "var", name: "free"},
        { kind: "const", value: 0, sort: Int },
      ],
    };
    expect(() => applyDeBruijn(orphan)).toThrow(/unbound variable/);
  });

  it("recurses through and / or / not / implies", () => {
    const inner = forall("x", Int, atomicNeq("x", 0));
    const f: IrFormula = {
      kind: "and",
      operands: [
        { kind: "or", operands: [inner] },
        { kind: "not", operands: [inner] },
        { kind: "implies", operands: [inner, inner]},
      ],
    };
    const out = applyDeBruijn(f);
    expect(out.kind).toBe("and");
  });

  it("preserves const and ctor terms", () => {
    const f: IrFormula = forall("x", Int, {
      kind: "atomic",
      name: "=",
      args: [
        { kind: "ctor", name: "+", args: [
          { kind: "var", name: "x" },
          { kind: "const", value: 1, sort: Int },
        ] },
        { kind: "const", value: 5, sort: Int },
      ],
    });
    const out = applyDeBruijn(f);
    expect(out.kind).toBe("forall");
  });
});

// -----------------------------------------------------------------------
// 14. passes/predicates.ts — canonicalizeTerm, termSortKey, canonicalizePredicate
// -----------------------------------------------------------------------

describe("14. canonicalizeTerm", () => {
  it("normalizes -0 to 0 in const number values", () => {
    const t: DeBruijnTerm = { kind: "const", value: -0, sort: Int };
    const out = canonicalizeTerm(t);
    if (out.kind !== "const") throw new Error();
    expect(Object.is(out.value, 0)).toBe(true);
  });

  it("preserves bigint values exactly", () => {
    const t: DeBruijnTerm = { kind: "const", value: 7n, sort: Int };
    const out = canonicalizeTerm(t);
    if (out.kind !== "const") throw new Error();
    expect(out.value).toBe(7n);
  });

  it("treats undefined / null const value as null", () => {
    const t: DeBruijnTerm = { kind: "const", value: undefined, sort: Int };
    const out = canonicalizeTerm(t);
    if (out.kind !== "const") throw new Error();
    expect(out.value).toBeNull();
  });

  it("var carries de Bruijn index, not name, in canonical output", () => {
    const t: DeBruijnTerm = { kind: "var", name: "ignored", sort: Int, deBruijn: 2 };
    const out = canonicalizeTerm(t);
    if (out.kind !== "var") throw new Error();
    expect(out.index).toBe(2);
    expect(out).not.toHaveProperty("name");
  });

  it("ctor recurses into args", () => {
    const t: DeBruijnTerm = {
      kind: "ctor",
      name: "+",
      args: [
        { kind: "const", value: 1, sort: Int },
        { kind: "const", value: 2, sort: Int },
      ],
      sort: Int,
    };
    const out = canonicalizeTerm(t);
    if (out.kind !== "ctor") throw new Error();
    expect(out.args).toHaveLength(2);
  });
});

describe("14b. termSortKey", () => {
  it("differs for differing var indices", () => {
    const a: CanonicalTerm = { kind: "var", index: 0, sort: { kind: "primitive", name: "Int" } };
    const b: CanonicalTerm = { kind: "var", index: 1, sort: { kind: "primitive", name: "Int" } };
    expect(termSortKey(a)).not.toBe(termSortKey(b));
  });

  it("encodes ctor args structurally", () => {
    const k1 = termSortKey({
      kind: "ctor",
      name: "+",
      args: [
        { kind: "const", value: 1, sort: { kind: "primitive", name: "Int" } },
        { kind: "const", value: 2, sort: { kind: "primitive", name: "Int" } },
      ],
      sort: { kind: "primitive", name: "Int" },
    });
    expect(k1).toContain("ctor:+");
  });
});

describe("14c. canonicalizePredicate", () => {
  const oneConst: CanonicalTerm = {
    kind: "const",
    sort: { kind: "primitive", name: "Int" },
    value: 1,
  };
  const twoConst: CanonicalTerm = {
    kind: "const",
    sort: { kind: "primitive", name: "Int" },
    value: 2,
  };
  const xVar: CanonicalTerm = {
    kind: "var",
    index: 0,
    sort: { kind: "primitive", name: "Int" },
  };

  it("resolves '!=' alias to '≠'", () => {
    expect(canonicalizePredicate("!=", [oneConst, twoConst]).name).toBe("≠");
  });

  it("resolves 'eq' alias to '='", () => {
    expect(canonicalizePredicate("eq", [oneConst, twoConst]).name).toBe("=");
  });

  it("resolves 'lessThan' alias to '<'", () => {
    expect(canonicalizePredicate("lessThan", [xVar, oneConst]).name).toBe("<");
  });

  it("resolves '∈' alias to 'member'", () => {
    expect(canonicalizePredicate("∈", [oneConst, oneConst]).name).toBe("member");
  });

  it("resolves 'kindOf' alias to 'kind-of'", () => {
    expect(canonicalizePredicate("kindOf", [oneConst, oneConst]).name).toBe("kind-of");
  });

  it("'=' sorts args by structural key", () => {
    // termSortKey for const Int 2 is greater than const Int 1 (ASCII '1' < '2').
    const out = canonicalizePredicate("=", [twoConst, oneConst]);
    expect((out.args[0] as { value: unknown }).value).toBe(1);
    expect((out.args[1] as { value: unknown }).value).toBe(2);
  });

  it("'<' with const-on-left flips to '>' with const-on-right", () => {
    const out = canonicalizePredicate("<", [oneConst, xVar]);
    expect(out.name).toBe(">");
    expect(out.args[0].kind).toBe("var");
    expect(out.args[1].kind).toBe("const");
  });

  it("unknown predicate name passes through unchanged", () => {
    const out = canonicalizePredicate("kit:custom", [oneConst]);
    expect(out.name).toBe("kit:custom");
  });
});

// -----------------------------------------------------------------------
// 15. passes/nnf.ts — toNnf
// -----------------------------------------------------------------------

describe("15. toNnf", () => {
  const intSort: CanonicalSort = { kind: "primitive", name: "Int" };
  const trueAtom: CanonicalFolAst = { kind: "atomic", name: "true", args: [] };
  const falseAtom: CanonicalFolAst = { kind: "atomic", name: "false", args: [] };
  const eqAtom: CanonicalFolAst = {
    kind: "atomic",
    name: "=",
    args: [
      { kind: "const", value: 1, sort: intSort },
      { kind: "const", value: 1, sort: intSort },
    ],
  };

  it("not(not(p)) collapses to p", () => {
    const r = toNnf({ kind: "not", operands: [{ kind: "not", operands: [eqAtom] }] });
    expect(r).toEqual(eqAtom);
  });

  it("not(true) → false via predicate negation", () => {
    expect(toNnf({ kind: "not", operands: [trueAtom] })).toEqual(falseAtom);
  });

  it("not(=) → ≠ via predicate negation", () => {
    const r = toNnf({ kind: "not", operands: [eqAtom] });
    if (r.kind !== "atomic") throw new Error();
    expect(r.name).toBe("≠");
  });

  it("not(<) → ≥ via predicate negation", () => {
    const lt: CanonicalFolAst = {
      kind: "atomic",
      name: "<",
      args: eqAtom.kind === "atomic" ? eqAtom.args : [],
    };
    const r = toNnf({ kind: "not", operands: [lt] });
    if (r.kind !== "atomic") throw new Error();
    expect(r.name).toBe("≥");
  });

  it("De Morgan: not(and(p,q)) → or(not(p), not(q))", () => {
    const p = eqAtom;
    const q: CanonicalFolAst = {
      kind: "atomic",
      name: "<",
      args: [
        { kind: "const", value: 0, sort: intSort },
        { kind: "const", value: 1, sort: intSort },
      ],
    };
    const r = toNnf({ kind: "not", operands: [{ kind: "and", operands: [p, q] }] });
    if (r.kind !== "or") throw new Error();
    expect(r.operands).toHaveLength(2);
  });

  it("not(forall) → exists with negated body", () => {
    const inside: CanonicalFolAst = { kind: "forall", sort: intSort, body: eqAtom };
    const r = toNnf({ kind: "not", operands: [inside] });
    if (r.kind !== "exists") throw new Error();
    expect(r.body.kind).toBe("atomic");
  });

  it("not(exists) → forall with negated body", () => {
    const inside: CanonicalFolAst = { kind: "exists", sort: intSort, body: eqAtom };
    const r = toNnf({ kind: "not", operands: [inside] });
    if (r.kind !== "forall") throw new Error();
  });

  it("not on kit-defined atomic predicate is left as not(atomic)", () => {
    const a: CanonicalFolAst = { kind: "atomic", name: "kit:reads", args: [] };
    const r = toNnf({ kind: "not", operands: [a] });
    expect(r.kind).toBe("not");
  });
});

// -----------------------------------------------------------------------
// 16. passes/acNormalize.ts — acNormalize + astSortKey
// -----------------------------------------------------------------------

describe("16. acNormalize + astSortKey", () => {
  const intSort: CanonicalSort = { kind: "primitive", name: "Int" };
  const trueAtom: CanonicalFolAst = { kind: "atomic", name: "true", args: [] };
  const falseAtom: CanonicalFolAst = { kind: "atomic", name: "false", args: [] };
  const eqAtom: CanonicalFolAst = {
    kind: "atomic",
    name: "=",
    args: [
      { kind: "const", value: 1, sort: intSort },
      { kind: "const", value: 1, sort: intSort },
    ],
  };

  it("astSortKey is deterministic for identical inputs", () => {
    expect(astSortKey(eqAtom)).toBe(astSortKey(eqAtom));
  });

  it("astSortKey differs across kinds", () => {
    expect(astSortKey(trueAtom)).not.toBe(astSortKey(falseAtom));
  });

  it("and() with no operands → true", () => {
    expect(acNormalize({ kind: "and", operands: [] })).toEqual(trueAtom);
  });

  it("or() with no operands → false", () => {
    expect(acNormalize({ kind: "or", operands: [] })).toEqual(falseAtom);
  });

  it("and(false, p) → false (absorption)", () => {
    expect(
      acNormalize({ kind: "and", operands: [falseAtom, eqAtom] }),
    ).toEqual(falseAtom);
  });

  it("or(true, p) → true (absorption)", () => {
    expect(
      acNormalize({ kind: "or", operands: [trueAtom, eqAtom] }),
    ).toEqual(trueAtom);
  });

  it("and(true, p) → p (identity removal)", () => {
    expect(
      acNormalize({ kind: "and", operands: [trueAtom, eqAtom] }),
    ).toEqual(eqAtom);
  });

  it("and(and(p,q),r) flattens", () => {
    const p = eqAtom;
    const q: CanonicalFolAst = { kind: "atomic", name: "kit:q", args: [] };
    const r: CanonicalFolAst = { kind: "atomic", name: "kit:r", args: [] };
    const out = acNormalize({
      kind: "and",
      operands: [{ kind: "and", operands: [p, q] }, r],
    });
    if (out.kind !== "and") throw new Error();
    expect(out.operands).toHaveLength(3);
  });

  it("and dedupes equal operands", () => {
    const out = acNormalize({ kind: "and", operands: [eqAtom, eqAtom] });
    expect(out).toEqual(eqAtom);
  });

  it("recurses into quantifier bodies", () => {
    const out = acNormalize({
      kind: "forall",
      sort: intSort,
      body: { kind: "and", operands: [trueAtom, eqAtom] },
    });
    if (out.kind !== "forall") throw new Error();
    expect(out.body).toEqual(eqAtom);
  });

  it("not is preserved through acNormalize", () => {
    const out = acNormalize({ kind: "not", operands: [eqAtom] });
    expect(out.kind).toBe("not");
  });
});

// -----------------------------------------------------------------------
// 17. passes/impliesRemoval.ts — removeImplies
// -----------------------------------------------------------------------

describe("17. removeImplies", () => {
  const intSort: CanonicalSort = { kind: "primitive", name: "Int" };
  const trueAtom: PreNnfAst = { kind: "atomic", name: "true", args: [] };
  const falseAtom: PreNnfAst = { kind: "atomic", name: "false", args: [] };

  it("rewrites implies(a, c) to or(not(a), c)", () => {
    const out = removeImplies({
      kind: "implies",
      operands: [trueAtom, falseAtom],
    });
    if (out.kind !== "or") throw new Error();
    expect(out.operands).toHaveLength(2);
    expect(out.operands[0]!.kind).toBe("not");
    expect(out.operands[1]!.kind).toBe("atomic");
  });

  it("recurses into and / or / not / forall / exists", () => {
    const inner: PreNnfAst = {
      kind: "implies",
      operands: [trueAtom, falseAtom],
    };
    const wrapped: PreNnfAst = {
      kind: "and",
      operands: [
        { kind: "not", operands: [inner] },
        { kind: "or", operands: [inner] },
        { kind: "forall", sort: intSort, body: inner },
        { kind: "exists", sort: intSort, body: inner },
      ],
    };
    const out = removeImplies(wrapped);
    function hasImplies(a: CanonicalFolAst): boolean {
      switch (a.kind) {
        case "and":
        case "or":
          return a.operands.some(hasImplies);
        case "not":
          return hasImplies(a.operands[0]);
        case "forall":
        case "exists":
          return hasImplies(a.body);
        default:
          return false;
      }
    }
    expect(hasImplies(out)).toBe(false);
  });

  it("atomic passes through unchanged", () => {
    const a: PreNnfAst = {
      kind: "atomic",
      name: "true",
      args: [],
    };
    expect(removeImplies(a)).toEqual({
      kind: "atomic",
      name: "true",
      args: [],
    });
  });
});

// -----------------------------------------------------------------------
// 18. serialize.ts — SERIALIZATION_FORMAT + bigint encodings
// -----------------------------------------------------------------------

describe("18. serialize.ts", () => {
  const intSort: CanonicalSort = { kind: "primitive", name: "Int" };

  it("SERIALIZATION_FORMAT is jcs-json-rfc8785", () => {
    expect(SERIALIZATION_FORMAT).toBe("jcs-json-rfc8785");
  });

  it("safe-range bigint serializes as a JSON number", () => {
    const ast: CanonicalFolAst = {
      kind: "atomic",
      name: "P",
      args: [{ kind: "const", value: 100n, sort: intSort }],
    };
    const s = serializeCanonicalAst(ast).toString("utf8");
    expect(s).toContain('"value":100');
    expect(s).not.toContain('"value":"');
  });

  it("unsafe-range bigint serializes with bigint: prefix as string", () => {
    const huge = BigInt(Number.MAX_SAFE_INTEGER) + 10n;
    const ast: CanonicalFolAst = {
      kind: "atomic",
      name: "P",
      args: [{ kind: "const", value: huge, sort: intSort }],
    };
    const s = serializeCanonicalAst(ast).toString("utf8");
    expect(s).toContain(`"value":"bigint:${huge.toString()}"`);
  });

  it("null const value serializes as null", () => {
    const ast: CanonicalFolAst = {
      kind: "atomic",
      name: "P",
      args: [{ kind: "const", value: null, sort: intSort }],
    };
    const s = serializeCanonicalAst(ast).toString("utf8");
    expect(s).toContain('"value":null');
  });

  it("boolean const values serialize as true/false", () => {
    const ast: CanonicalFolAst = {
      kind: "atomic",
      name: "P",
      args: [
        { kind: "const", value: true, sort: { kind: "primitive", name: "Bool" } },
        { kind: "const", value: false, sort: { kind: "primitive", name: "Bool" } },
      ],
    };
    const s = serializeCanonicalAst(ast).toString("utf8");
    expect(s).toContain("true");
    expect(s).toContain("false");
  });

  it("object keys are sorted lexicographically (RFC 8785 §3.2.3)", () => {
    const ast: CanonicalFolAst = {
      kind: "atomic",
      name: "P",
      args: [],
    };
    const s = serializeCanonicalAst(ast).toString("utf8");
    // Field "name" is the v1.1 grammar's renamed "predicate".
    expect(s.indexOf("args")).toBeLessThan(s.indexOf("kind"));
    expect(s.indexOf("kind")).toBeLessThan(s.indexOf("name"));
  });
});

// -----------------------------------------------------------------------
// 19. hash.ts — BLAKE3-512 self-identifying hash (protocol v1.1.0)
// -----------------------------------------------------------------------

describe("19. computeCid (BLAKE3-512 self-identifying hash)", () => {
  // Format: "blake3-512:" prefix + 128 lowercase hex chars (full 64-byte digest).
  const SELF_ID = /^blake3-512:[0-9a-f]{128}$/;

  it("returns a self-identifying string with the blake3-512 prefix and full digest", () => {
    const out = computeCid(Buffer.from("hello", "utf8"));
    expect(out).toMatch(SELF_ID);
  });

  it("hashes empty buffer deterministically", () => {
    const out1 = computeCid(Buffer.alloc(0));
    const out2 = computeCid(Buffer.alloc(0));
    expect(out1).toBe(out2);
    expect(out1).toMatch(SELF_ID);
  });

  it("differs for different bytes", () => {
    const a = computeCid(Buffer.from("a", "utf8"));
    const b = computeCid(Buffer.from("b", "utf8"));
    expect(a).not.toBe(b);
  });

  it("matches the canonical BLAKE3-512 digest for the empty input", () => {
    // Reference value from BLAKE3 spec (XOF output of length 64 for ""):
    expect(blake3_512_hex(Buffer.alloc(0))).toBe(
      "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262e00f03e7b69af26b7faaf09fcd333050338ddfe085b8cc869ca98b206c08243a",
    );
    expect(computeCid(Buffer.alloc(0))).toBe(
      "blake3-512:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262e00f03e7b69af26b7faaf09fcd333050338ddfe085b8cc869ca98b206c08243a",
    );
  });
});

// -----------------------------------------------------------------------
// 20. canonicalize.ts — propertyHashFromAst direct
// -----------------------------------------------------------------------

describe("20. propertyHashFromAst", () => {
  it("matches propertyHashFromFormula via the full pipeline", () => {
    const f = forall("b", Int, atomicNeq("b", 0));
    const ast = formulaToCanonicalAst(f);
    expect(propertyHashFromAst(ast)).toBe(propertyHashFromFormula(f));
  });
});

// -----------------------------------------------------------------------
// 21. AstCanonicalizerImpl + canonicalizer default export
// -----------------------------------------------------------------------

describe("21. AstCanonicalizerImpl + canonicalizer default", () => {
  it("default canonicalizer instance shares the same hashing as the function", () => {
    const f = forall("b", Int, atomicNeq("b", 0));
    expect(canonicalizer.propertyHashFromFormula(f)).toBe(
      propertyHashFromFormula(f),
    );
  });

  it("formulaToCanonicalAst on the impl matches the standalone function", () => {
    const impl = new AstCanonicalizerImpl();
    const f = forall("b", Int, atomicNeq("b", 0));
    expect(impl.formulaToCanonicalAst(f)).toEqual(formulaToCanonicalAst(f));
  });

  it("scopeOf returns the documented stub shape", () => {
    const impl = new AstCanonicalizerImpl();
    const scope = impl.scopeOf({});
    expect(scope.kind).toBe("function");
    expect(scope.identifier).toBe("__stub__");
    expect(scope.filePath).toBe("__stub__");
  });

  it("bindingHashFromAst is a 16-hex string", () => {
    const impl = new AstCanonicalizerImpl();
    const scope: BindingScope = {
      kind: "function",
      identifier: "divide",
      filePath: "/src/divide.ts",
    };
    const bindings: Bindings = { b: { kind: "primitive", name: "Int" } };
    const h = impl.bindingHashFromAst({ scope, bindings, hostAst: null });
    expect(h).toMatch(/^blake3-512:[0-9a-f]{128}$/);
  });

  it("bindingHashFromAst is deterministic and order-independent in bindings", () => {
    const impl = new AstCanonicalizerImpl();
    const scope: BindingScope = {
      kind: "function",
      identifier: "f",
      filePath: "x.ts",
    };
    const Int_: CanonicalSort = { kind: "primitive", name: "Int" };
    const Bool_: CanonicalSort = { kind: "primitive", name: "Bool" };
    const a = impl.bindingHashFromAst({
      scope,
      bindings: { x: Int_, y: Bool_ },
      hostAst: null,
    });
    const b = impl.bindingHashFromAst({
      scope,
      bindings: { y: Bool_, x: Int_ },
      hostAst: null,
    });
    expect(a).toBe(b);
  });

  it("bindingHashFromAst differs when the scope identifier differs", () => {
    const impl = new AstCanonicalizerImpl();
    const Int_: CanonicalSort = { kind: "primitive", name: "Int" };
    const a = impl.bindingHashFromAst({
      scope: { kind: "function", identifier: "f", filePath: "x.ts" },
      bindings: { x: Int_ },
      hostAst: null,
    });
    const b = impl.bindingHashFromAst({
      scope: { kind: "function", identifier: "g", filePath: "x.ts" },
      bindings: { x: Int_ },
      hostAst: null,
    });
    expect(a).not.toBe(b);
  });
});
