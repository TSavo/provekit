import { test, expect, beforeEach } from "vitest";
import {
  property,
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
  expect(parseInt(str("0"))).toEqual({
    kind: "ctor",
    name: "parseInt",
    args: [{ kind: "const", value: "0", sort: StringSort }],
    sort: Int,
  });
});

test("Math.abs preserves the input's sort", () => {
  const t = abs(num(-3));
  expect(t.sort).toEqual(Int);
  if (t.kind === "ctor") expect(t.name).toBe("Math.abs");
});

test("isFinite returns a Bool-typed term", () => {
  const t = isFinite(num(1));
  if (t.kind === "ctor") expect(t.sort.kind).toBe("primitive");
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
    sort: Int,
  });
});

// ---------------------------------------------------------------------------
// atomic predicates
// ---------------------------------------------------------------------------

test("eq builds an atomic = formula", () => {
  expect(eq(num(0), num(0))).toEqual({
    kind: "atomic",
    predicate: "=",
    args: [
      { kind: "const", value: 0, sort: Int },
      { kind: "const", value: 0, sort: Int },
    ],
  });
});

test("gt builds an atomic > formula with mixed liftable args", () => {
  const f = gt(parseInt(str("0")), 0);
  if (f.kind === "atomic") {
    expect(f.predicate).toBe(">");
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
  expect(decls[0]!.kind).toBe("property");
  if (decls[0]!.kind === "property") {
    expect(decls[0]!.name).toBe("zeroIsZero");
    expect(decls[0]!.formula.kind).toBe("atomic");
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
  expect(decls.map((d) => d.kind)).toEqual(["property", "bridge", "property"]);
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
    expect(f.predicate.body.kind).toBe("atomic");
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
  expect(t.sort).toEqual(Real);
});

test("isNaN / isInteger return Bool-typed ctor terms", () => {
  const a = isNaN(num(0));
  const b = isInteger(num(0));
  if (a.kind !== "ctor" || b.kind !== "ctor") throw new Error();
  expect(a.sort).toEqual(Bool);
  expect(a.name).toBe("isNaN");
  expect(b.name).toBe("isInteger");
});

test("max / min preserve the first argument's sort", () => {
  const t = max(num(1), num(2));
  if (t.kind !== "ctor") throw new Error();
  expect(t.name).toBe("Math.max");
  expect(t.sort).toEqual(Int);
  const t2 = min(real(1.5), real(2.5));
  if (t2.kind !== "ctor") throw new Error();
  expect(t2.sort).toEqual(Real);
});

test("floor / ceil / sign return Int", () => {
  const f = floor(real(1.5));
  const c = ceil(real(1.5));
  const s = sign(num(-3));
  if (f.kind !== "ctor" || c.kind !== "ctor" || s.kind !== "ctor") throw new Error();
  expect(f.sort).toEqual(Int);
  expect(c.sort).toEqual(Int);
  expect(s.sort).toEqual(Int);
});

test("sqrt returns Real regardless of input sort", () => {
  const t = sqrt(num(4));
  if (t.kind !== "ctor") throw new Error();
  expect(t.sort).toEqual(Real);
  expect(t.name).toBe("Math.sqrt");
});

test("stringLength / stringIncludes / arrayLength / arrayIncludes have correct sorts", () => {
  const sLen = stringLength(str("hi"));
  const sInc = stringIncludes(str("hi"), str("h"));
  const aLen = arrayLength(str("[]"));
  const aInc = arrayIncludes(str("[]"), num(0));
  if (sLen.kind !== "ctor" || sInc.kind !== "ctor" || aLen.kind !== "ctor" || aInc.kind !== "ctor") throw new Error();
  expect(sLen.sort).toEqual(Int);
  expect(sInc.sort).toEqual(Bool);
  expect(aLen.sort).toEqual(Int);
  expect(aInc.sort).toEqual(Bool);
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
  expect(d.sort).toEqual(Real);
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
    const f = fn(0, 1) as { kind: "atomic"; predicate: string };
    expect(f.predicate).toBe(predicate);
  }
});

test("isTrue / isFalse build atomics on Bool-typed args", () => {
  const t = isTrue(true);
  const f = isFalse(false);
  if (t.kind !== "atomic" || f.kind !== "atomic") throw new Error();
  expect(t.predicate).toBe("true");
  expect(f.predicate).toBe("false");
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
  // Don't call finish — simulate an exception leaking the active collector.
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
  expect(decl.kind).toBe("property");
  if (decl.kind === "property") {
    expect(decl.name).toBe("parseInt > canReturnZero");
    expect(decl.formula.kind).toBe("exists");
    if (decl.formula.kind === "exists") {
      expect(decl.formula.sort).toEqual(StringSort);
    }
  }
});
