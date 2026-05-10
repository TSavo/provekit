import { test, expect, beforeEach, describe as vDescribe } from "vitest";
import { inferSortHint } from "./primitives.js";
import type { IrTerm } from "../formulas.js";
const property = (name: string, formula: import("../formulas.js").IrFormula) =>
  contract(name, { pre: formula });
function termSort(t: IrTerm) {
  return inferSortHint(t);
}
import {
  contract,
  bridge,
  describe,
  must,
  beginCollecting,
  _resetCollector,
  forAll,
  exists,
  parseInt,
  parseFloat,
  num,
  real,
  str,
  bool,
  eq,
  neq,
  lt,
  lte,
  gt,
  gte,
  isTrue,
  isFalse,
  Int,
  Real,
  Bool,
  String as StringSort,
  BV,
  BV8,
  BV16,
  BV32,
  bv,
  bvadd,
  bvsub,
  bvmul,
  bvudiv,
  bvurem,
  bvshl,
  bvlshr,
  bvashr,
  bvor,
  bvand,
  bvxor,
  bvnot,
  bvneg,
  concat,
  extract,
  bvult,
  bvule,
  bvugt,
  bvuge,
  bvslt,
  bvsle,
  bvsgt,
  bvsge,
  abs,
  max,
  min,
  floor,
  ceil,
  sqrt,
  sign,
  add,
  sub,
  mul,
  div,
  neg,
  isFinite,
  isNaN,
  isInteger,
  stringLength,
  stringIncludes,
  arrayLength,
  arrayIncludes,
  and,
  or,
  not,
  implies,
  iff,
  lambda,
  letTerm,
  choice,
  type Declaration,
} from "./index.js";

beforeEach(() => {
  _resetCollector();
});

// ---------------------------------------------------------------------------
// constants
// ---------------------------------------------------------------------------

test("num builds an Int constant", () => {
  expect(num(42)).toEqual({ kind: "const", value: 42, sort: Int });
});

test("str builds a String constant", () => {
  expect(str("0")).toEqual({ kind: "const", value: "0", sort: StringSort });
});

// ---------------------------------------------------------------------------
// built-in function primitives
// ---------------------------------------------------------------------------

test("parseInt builds an apply ctor over its argument", () => {
  const t = parseInt(str("0"));
  // Spec v1.1: ctor terms carry no sort field on the wire; the kit
  // tracks return sort via a non-enumerable side channel (recoverable
  // through inferSortHint).
  expect(t).toEqual({
    kind: "ctor",
    name: "parseInt",
    args: [{ kind: "const", value: "0", sort: StringSort }],
  });
  expect(termSort(t)).toEqual(Int);
});

test("Math.abs preserves the input's sort", () => {
  const t = abs(num(-3));
  expect(termSort(t)).toEqual(Int);
  if (t.kind === "ctor") expect(t.name).toBe("Math.abs");
});

test("isFinite returns a Bool-typed term", () => {
  const t = isFinite(num(1));
  if (t.kind === "ctor") expect(termSort(t)?.kind).toBe("primitive");
});

// ---------------------------------------------------------------------------
// term arithmetic
// ---------------------------------------------------------------------------

test("add over numbers lifts to const terms", () => {
  expect(add(2, 3)).toEqual({
    kind: "ctor",
    name: "+",
    args: [
      { kind: "const", value: 2, sort: Int },
      { kind: "const", value: 3, sort: Int },
    ],
  });
});

// ---------------------------------------------------------------------------
// atomic predicates
// ---------------------------------------------------------------------------

test("eq builds an atomic = formula", () => {
  expect(eq(num(0), num(0))).toEqual({
    kind: "atomic",
    name: "=",
    args: [
      { kind: "const", value: 0, sort: Int },
      { kind: "const", value: 0, sort: Int },
    ],
  });
});

test("gt builds an atomic > formula with mixed liftable args", () => {
  const f = gt(parseInt(str("0")), 0);
  if (f.kind === "atomic") {
    expect(f.name).toBe(">");
    expect(f.args).toHaveLength(2);
  }
});

// ---------------------------------------------------------------------------
// property() + bridge() collection
// ---------------------------------------------------------------------------

test("property() collects a PropertyDeclaration", () => {
  const finish = beginCollecting();
  property("zeroIsZero", eq(parseInt(str("0")), num(0)));
  const decls = finish();
  expect(decls).toHaveLength(1);
  expect(decls[0]!.kind).toBe("contract");
  if (decls[0]!.kind === "contract") {
    expect(decls[0]!.name).toBe("zeroIsZero");
    expect(decls[0]!.pre?.kind).toBe("atomic");
  }
});

test("bridge() collects a BridgeDeclaration", () => {
  const finish = beginCollecting();
  bridge("parseIntBridgesV8", {
    sourceSymbol: "global.parseInt",
    sourceLayer: "ts-kit@1.0",
    targetContractCid: "abc1234567890def",
    targetLayer: "V8@12.4",
    notes: "the canonical bridge",
  });
  const decls = finish();
  expect(decls).toHaveLength(1);
  const d = decls[0] as Declaration;
  expect(d.kind).toBe("bridge");
  if (d.kind === "bridge") {
    expect(d.sourceSymbol).toBe("global.parseInt");
    expect(d.targetContractCid).toBe("abc1234567890def");
    expect(d.notes).toBe("the canonical bridge");
  }
});

test("multiple property + bridge calls collect in order", () => {
  const finish = beginCollecting();
  property("p1", eq(num(0), num(0)));
  bridge("b1", {
    sourceSymbol: "x",
    sourceLayer: "L1",
    targetContractCid: "0".repeat(32),
    targetLayer: "L2",
  });
  property("p2", eq(num(1), num(1)));
  const decls = finish();
  expect(decls.map((d) => d.kind)).toEqual(["contract", "bridge", "contract"]);
  expect(decls.map((d) => d.name)).toEqual(["p1", "b1", "p2"]);
});

test("calling property without an active collector throws", () => {
  expect(() => property("x", eq(num(0), num(0)))).toThrow(/outside an active collector/);
});

test("nested beginCollecting throws", () => {
  const finish = beginCollecting();
  try {
    expect(() => beginCollecting()).toThrow(/already active/);
  } finally {
    finish();
  }
});

// ---------------------------------------------------------------------------
// quantifiers
// ---------------------------------------------------------------------------

test("forAll wraps a body builder", () => {
  const f = forAll(Int, (x) => gt(x, num(0)));
  expect(f.kind).toBe("forall");
  if (f.kind === "forall") {
    expect(f.sort).toEqual(Int);
    expect(f.body.kind).toBe("atomic");
  }
});

test("exists wraps a body builder", () => {
  const f = exists(StringSort, (s) => eq(parseInt(s), num(0)));
  expect(f.kind).toBe("exists");
});

test("nested quantifiers compose", () => {
  const f = forAll(Int, (x) => forAll(Int, (y) => gt(add(x, y), num(0))));
  expect(f.kind).toBe("forall");
});

// ---------------------------------------------------------------------------
// describe() + must() ergonomics
// ---------------------------------------------------------------------------

test("describe + must registers a single named invariant", () => {
  const finish = beginCollecting();
  describe("parseInt", () => {
    must("canReturnZero", exists(StringSort, (s) => eq(parseInt(s), num(0))));
  });
  const decls = finish();
  expect(decls).toHaveLength(1);
  expect(decls[0]!.name).toBe("parseInt > canReturnZero");
});

test("nested describes build a path", () => {
  const finish = beginCollecting();
  describe("Math", () => {
    describe("abs", () => {
      must("non-negative", forAll(Int, (x) => gt(abs(x), num(-1))));
    });
  });
  const decls = finish();
  expect(decls).toHaveLength(1);
  expect(decls[0]!.name).toBe("Math > abs > non-negative");
});

test("multiple must() in one describe", () => {
  const finish = beginCollecting();
  describe("parseInt", () => {
    must("canReturnZero", eq(num(0), num(0)));
    must("canReturnPositive", eq(num(1), num(1)));
  });
  const decls = finish();
  expect(decls).toHaveLength(2);
  expect(decls.map((d) => d.name)).toEqual([
    "parseInt > canReturnZero",
    "parseInt > canReturnPositive",
  ]);
});

test("describe pops its segment after body returns", () => {
  const finish = beginCollecting();
  describe("a", () => {
    must("inner", eq(num(0), num(0)));
  });
  must("outer", eq(num(1), num(1)));
  const decls = finish();
  expect(decls.map((d) => d.name)).toEqual(["a > inner", "outer"]);
});

// ---------------------------------------------------------------------------
// must.skip
// ---------------------------------------------------------------------------

test("must.skip is a no-op", () => {
  const finish = beginCollecting();
  describe("parseInt", () => {
    must.skip("legacy invariant", eq(num(0), num(0)));
    must("real invariant", eq(num(1), num(1)));
  });
  const decls = finish();
  expect(decls).toHaveLength(1);
  expect(decls[0]!.name).toBe("parseInt > real invariant");
});

// ---------------------------------------------------------------------------
// the worked example
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// remaining constants (real, bool) and integer/non-integer detection in num
// ---------------------------------------------------------------------------

test("num builds a Real constant when value is non-integer", () => {
  expect(num(1.5)).toEqual({ kind: "const", value: 1.5, sort: Real });
});

test("num accepts a bigint and tags it as Int", () => {
  expect(num(7n)).toEqual({ kind: "const", value: 7n, sort: Int });
});

test("real always builds a Real constant", () => {
  expect(real(3)).toEqual({ kind: "const", value: 3, sort: Real });
});

test("bool builds a Bool constant", () => {
  expect(bool(true)).toEqual({ kind: "const", value: true, sort: Bool });
  expect(bool(false)).toEqual({ kind: "const", value: false, sort: Bool });
});

// ---------------------------------------------------------------------------
// remaining built-in function primitives
// ---------------------------------------------------------------------------

test("parseFloat builds an apply ctor returning Real", () => {
  const t = parseFloat(str("0.5"));
  if (t.kind !== "ctor") throw new Error();
  expect(t.name).toBe("parseFloat");
  expect(termSort(t)).toEqual(Real);
});

test("isNaN / isInteger return Bool-typed ctor terms", () => {
  const a = isNaN(num(0));
  const b = isInteger(num(0));
  if (a.kind !== "ctor" || b.kind !== "ctor") throw new Error();
  expect(termSort(a)).toEqual(Bool);
  expect(a.name).toBe("isNaN");
  expect(b.name).toBe("isInteger");
});

test("max / min preserve the first argument's sort", () => {
  const t = max(num(1), num(2));
  if (t.kind !== "ctor") throw new Error();
  expect(t.name).toBe("Math.max");
  expect(termSort(t)).toEqual(Int);
  const t2 = min(real(1.5), real(2.5));
  if (t2.kind !== "ctor") throw new Error();
  expect(termSort(t2)).toEqual(Real);
});

test("floor / ceil / sign return Int", () => {
  const f = floor(real(1.5));
  const c = ceil(real(1.5));
  const s = sign(num(-3));
  if (f.kind !== "ctor" || c.kind !== "ctor" || s.kind !== "ctor") throw new Error();
  expect(termSort(f)).toEqual(Int);
  expect(termSort(c)).toEqual(Int);
  expect(termSort(s)).toEqual(Int);
});

test("sqrt returns Real regardless of input sort", () => {
  const t = sqrt(num(4));
  if (t.kind !== "ctor") throw new Error();
  expect(termSort(t)).toEqual(Real);
  expect(t.name).toBe("Math.sqrt");
});

test("stringLength / stringIncludes / arrayLength / arrayIncludes have correct sorts", () => {
  const sLen = stringLength(str("hi"));
  const sInc = stringIncludes(str("hi"), str("h"));
  const aLen = arrayLength(str("[]"));
  const aInc = arrayIncludes(str("[]"), num(0));
  if (sLen.kind !== "ctor" || sInc.kind !== "ctor" || aLen.kind !== "ctor" || aInc.kind !== "ctor") throw new Error();
  expect(termSort(sLen)).toEqual(Int);
  expect(termSort(sInc)).toEqual(Bool);
  expect(termSort(aLen)).toEqual(Int);
  expect(termSort(aInc)).toEqual(Bool);
});

// ---------------------------------------------------------------------------
// remaining arithmetic
// ---------------------------------------------------------------------------

test("sub / mul build their respective ctors", () => {
  const s = sub(5, 3);
  const m = mul(2, 4);
  if (s.kind !== "ctor" || m.kind !== "ctor") throw new Error();
  expect(s.name).toBe("-");
  expect(m.name).toBe("*");
});

test("div produces a Real-typed term", () => {
  const d = div(num(1), num(2));
  if (d.kind !== "ctor") throw new Error();
  expect(d.name).toBe("/");
  expect(termSort(d)).toEqual(Real);
});

test("neg produces a unary - ctor", () => {
  const n = neg(num(5));
  if (n.kind !== "ctor") throw new Error();
  expect(n.name).toBe("-");
  expect(n.args).toHaveLength(1);
});

// ---------------------------------------------------------------------------
// remaining atomic predicates
// ---------------------------------------------------------------------------

test("neq / lt / lte / gte build atomics with the right predicate names", () => {
  expect(neq(0, 1).kind).toBe("atomic");
  const tests: Array<[(a: number, b: number) => unknown, string]> = [
    [neq, "≠"],
    [lt, "<"],
    [lte, "≤"],
    [gte, "≥"],
  ];
  for (const [fn, predicate] of tests) {
    const f = fn(0, 1) as { kind: "atomic"; name: string };
    expect(f.name).toBe(predicate);
  }
});

test("isTrue / isFalse build atomics on Bool-typed args", () => {
  const t = isTrue(true);
  const f = isFalse(false);
  if (t.kind !== "atomic" || f.kind !== "atomic") throw new Error();
  expect(t.name).toBe("true");
  expect(f.name).toBe("false");
});

// ---------------------------------------------------------------------------
// connectives re-export
// ---------------------------------------------------------------------------

test("symbolic and/or/not/implies/iff are re-exported correctly", () => {
  const a = eq(num(0), num(0));
  const b = eq(num(1), num(1));
  expect(and(a, b).kind).toBe("and");
  expect(or(a, b).kind).toBe("or");
  expect(not(a).kind).toBe("not");
  expect(implies(a, b).kind).toBe("implies");
  // iff desugars to and(implies, implies)
  expect(iff(a, b).kind).toBe("and");
});

// ---------------------------------------------------------------------------
// describe.skip
// ---------------------------------------------------------------------------

test("describe.skip is a no-op (its body is not invoked)", () => {
  const finish = beginCollecting();
  let bodyRan = false;
  describe.skip("never", () => {
    bodyRan = true;
    must("ignored", eq(num(0), num(0)));
  });
  const decls = finish();
  expect(decls).toHaveLength(0);
  expect(bodyRan).toBe(false);
});

// ---------------------------------------------------------------------------
// bridge() rejection outside collector
// ---------------------------------------------------------------------------

test("bridge() outside an active collector throws", () => {
  expect(() =>
    bridge("orphan", {
      sourceSymbol: "x",
      sourceLayer: "L1",
      targetContractCid: "0".repeat(32),
      targetLayer: "L2",
    }),
  ).toThrow(/outside an active collector/);
});

// ---------------------------------------------------------------------------
// _resetCollector clears in-progress state
// ---------------------------------------------------------------------------

test("_resetCollector lets a new beginCollecting succeed even if previous was leaked", () => {
  beginCollecting();
  // Don't call finish: simulate an exception leaking the active collector.
  _resetCollector();
  // Now this must NOT throw "already active".
  const finish = beginCollecting();
  property("ok", eq(num(0), num(0)));
  const decls = finish();
  expect(decls).toHaveLength(1);
});

test("worked example: parseInt-can-return-zero composes via runtime evaluation", () => {
  const finish = beginCollecting();
  describe("parseInt", () => {
    must("canReturnZero", exists(StringSort, (s) => eq(parseInt(s), num(0))));
  });
  const decls = finish();
  expect(decls).toHaveLength(1);
  const decl = decls[0]!;
  expect(decl.kind).toBe("contract");
  if (decl.kind === "contract") {
    expect(decl.name).toBe("parseInt > canReturnZero");
    if (!decl.pre) throw new Error("expected pre formula");
    expect(decl.pre.kind).toBe("exists");
    if (decl.pre.kind === "exists") {
      expect(decl.pre.sort).toEqual(StringSort);
    }
  }
});

// ---------------------------------------------------------------------------
// Bitvector primitives
// ---------------------------------------------------------------------------

vDescribe("BV sort builders", () => {
  test("BV(width) constructs a bitvec sort", () => {
    expect(BV(8)).toEqual({ kind: "bitvec", width: 8 });
    expect(BV(32)).toEqual({ kind: "bitvec", width: 32 });
  });

  test("named BV singletons (BV8, BV16, BV32) carry the correct width", () => {
    expect(BV8).toEqual({ kind: "bitvec", width: 8 });
    expect(BV16).toEqual({ kind: "bitvec", width: 16 });
    expect(BV32).toEqual({ kind: "bitvec", width: 32 });
  });

  test("BV() rejects non-positive or non-integer widths", () => {
    expect(() => BV(0)).toThrow(/positive integer/);
    expect(() => BV(-1)).toThrow(/positive integer/);
    expect(() => BV(1.5)).toThrow(/positive integer/);
  });
});

vDescribe("bv constants", () => {
  test("bv(value, width) builds a const term tagged with the BV sort", () => {
    expect(bv(7, 8)).toEqual({
      kind: "const",
      value: 7n,
      sort: { kind: "bitvec", width: 8 },
    });
  });

  test("bv() normalizes negative inputs into the unsigned bit range", () => {
    expect(bv(-1, 8)).toEqual({
      kind: "const",
      value: 255n,
      sort: { kind: "bitvec", width: 8 },
    });
  });

  test("bv() accepts a bigint value", () => {
    expect(bv(10n, 16)).toEqual({
      kind: "const",
      value: 10n,
      sort: { kind: "bitvec", width: 16 },
    });
  });

  test("bv() rejects a non-positive width", () => {
    expect(() => bv(1, 0)).toThrow(/width/);
  });
});

vDescribe("BV term operators preserve operand width", () => {
  const x = bv(0, 32);
  const y = bv(0, 32);

  test("bvadd / bvsub / bvmul / bvudiv / bvurem return BV<width>", () => {
    for (const op of [bvadd, bvsub, bvmul, bvudiv, bvurem]) {
      const t = op(x, y);
      expect(t.kind).toBe("ctor");
      expect(termSort(t)).toEqual({ kind: "bitvec", width: 32 });
    }
  });

  test("bvshl / bvlshr / bvashr return BV<width>", () => {
    for (const op of [bvshl, bvlshr, bvashr]) {
      const t = op(x, y);
      expect(termSort(t)).toEqual({ kind: "bitvec", width: 32 });
    }
  });

  test("bvand / bvor / bvxor return BV<width>", () => {
    for (const op of [bvand, bvor, bvxor]) {
      const t = op(x, y);
      expect(termSort(t)).toEqual({ kind: "bitvec", width: 32 });
    }
  });

  test("bvnot / bvneg are unary and preserve width", () => {
    expect(termSort(bvnot(x))).toEqual({ kind: "bitvec", width: 32 });
    expect(termSort(bvneg(x))).toEqual({ kind: "bitvec", width: 32 });
  });

  test("bvadd uses the ctor name 'bvadd'", () => {
    const t = bvadd(x, y);
    if (t.kind !== "ctor") throw new Error();
    expect(t.name).toBe("bvadd");
  });

  test("bvadd rejects mismatched widths", () => {
    expect(() => bvadd(bv(0, 8), bv(0, 16))).toThrow(/widths/);
  });
});

vDescribe("concat and extract widths", () => {
  test("concat(a, b) returns BV<wa + wb>", () => {
    const t = concat(bv(0, 8), bv(0, 16));
    expect(termSort(t)).toEqual({ kind: "bitvec", width: 24 });
  });

  test("extract(hi, lo, x) returns BV<hi - lo + 1>", () => {
    const t = extract(7, 0, bv(0, 32));
    expect(termSort(t)).toEqual({ kind: "bitvec", width: 8 });
  });

  test("extract encodes hi and lo as Int constants in args", () => {
    const t = extract(15, 4, bv(0, 32));
    if (t.kind !== "ctor") throw new Error();
    expect(t.name).toBe("extract");
    expect(t.args).toHaveLength(3);
    expect(t.args[0]).toEqual({ kind: "const", value: 15n, sort: Int });
    expect(t.args[1]).toEqual({ kind: "const", value: 4n, sort: Int });
  });

  test("extract rejects out-of-range indices", () => {
    expect(() => extract(32, 0, bv(0, 32))).toThrow(/range/);
    expect(() => extract(7, -1, bv(0, 32))).toThrow(/range/);
    expect(() => extract(3, 7, bv(0, 32))).toThrow(/range/);
  });
});

vDescribe("BV comparison predicates", () => {
  const x = bv(0, 8);
  const y = bv(0, 8);

  test("each comparison builds an atomic with the matching predicate name", () => {
    const cases: Array<[(a: typeof x, b: typeof y) => unknown, string]> = [
      [bvult, "bvult"],
      [bvule, "bvule"],
      [bvugt, "bvugt"],
      [bvuge, "bvuge"],
      [bvslt, "bvslt"],
      [bvsle, "bvsle"],
      [bvsgt, "bvsgt"],
      [bvsge, "bvsge"],
    ];
    for (const [fn, predicate] of cases) {
      const f = fn(x, y) as { kind: "atomic"; name: string };
      expect(f.kind).toBe("atomic");
      expect(f.name).toBe(predicate);
    }
  });

  test("comparison rejects mismatched widths", () => {
    expect(() => bvult(bv(0, 8), bv(0, 16))).toThrow(/widths/);
  });
});

vDescribe("BV with quantifiers", () => {
  test("forAll over BV32 produces an IR with the bitvec sort on the quantifier", () => {
    const f = forAll(BV32, (x) => eq(bvxor(x, x), bv(0, 32)));
    expect(f.kind).toBe("forall");
    if (f.kind === "forall") {
      expect(f.sort).toEqual({ kind: "bitvec", width: 32 });
      expect(f.body.kind).toBe("atomic");
    }
  });
});

// ---------------------------------------------------------------------------
// Lambda terms
// ---------------------------------------------------------------------------

vDescribe("lambda", () => {
  test("lambda builds a lambda term with paramName, paramSort, body", () => {
    const lam = lambda("x", Int, num(42));
    expect(lam.kind).toBe("lambda");
    if (lam.kind === "lambda") {
      expect(lam.paramName).toBe("x");
      expect(lam.paramSort).toEqual(Int);
      expect(lam.body).toEqual(num(42));
    }
  });

  test("lambda infers function sort from param and body", () => {
    const lam = lambda("x", Int, num(42));
    expect(termSort(lam)).toEqual({ kind: "function", args: [Int], return: Int });
  });
});

// ---------------------------------------------------------------------------
// Let terms
// ---------------------------------------------------------------------------

vDescribe("let", () => {
  test("letTerm builds a let with bindings and body", () => {
    const l = letTerm([{ name: "x", boundTerm: num(1) }], num(2));
    expect(l.kind).toBe("let");
    if (l.kind === "let") {
      expect(l.bindings).toHaveLength(1);
      expect(l.bindings[0]!.name).toBe("x");
      expect(l.bindings[0]!.boundTerm).toEqual(num(1));
      expect(l.body).toEqual(num(2));
    }
  });

  test("letTerm infers sort from body", () => {
    const l = letTerm([{ name: "x", boundTerm: num(1) }], num(2));
    expect(termSort(l)).toEqual(Int);
  });
});

// ---------------------------------------------------------------------------
// Choice formulas
// ---------------------------------------------------------------------------

vDescribe("choice", () => {
  test("choice builds a choice formula with varName, sort, body", () => {
    const body = eq({ kind: "var", name: "x" }, num(0));
    const c = choice("x", Int, body);
    expect(c.kind).toBe("choice");
    if (c.kind === "choice") {
      expect(c.varName).toBe("x");
      expect(c.sort).toEqual(Int);
      expect(c.body.kind).toBe("atomic");
    }
  });
});
