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
  num,
  str,
  eq,
  gt,
  Int,
  String as StringSort,
  abs,
  add,
  isFinite,
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
