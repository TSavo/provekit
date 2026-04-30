import { describe, it, expect, beforeEach } from "vitest";
import {
  property,
  bridge,
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

describe("symbolic primitives", () => {
  beforeEach(() => {
    _resetCollector();
  });

  describe("constants", () => {
    it("num builds an Int constant", () => {
      const n = num(42);
      expect(n).toEqual({ kind: "const", value: 42, sort: Int });
    });

    it("str builds a String constant", () => {
      const s = str("0");
      expect(s).toEqual({ kind: "const", value: "0", sort: StringSort });
    });
  });

  describe("built-in function primitives", () => {
    it("parseInt builds an apply ctor over its argument", () => {
      const t = parseInt(str("0"));
      expect(t).toEqual({
        kind: "ctor",
        name: "parseInt",
        args: [{ kind: "const", value: "0", sort: StringSort }],
        sort: Int,
      });
    });

    it("Math.abs preserves the input's sort", () => {
      const t = abs(num(-3));
      expect(t.sort).toEqual(Int);
      if (t.kind === "ctor") {
        expect(t.name).toBe("Math.abs");
      }
    });

    it("isFinite returns a Bool-typed term", () => {
      const t = isFinite(num(1));
      if (t.kind === "ctor") {
        expect(t.sort.kind).toBe("primitive");
      }
    });
  });

  describe("term arithmetic", () => {
    it("add over numbers lifts to const terms", () => {
      const t = add(2, 3);
      expect(t).toEqual({
        kind: "ctor",
        name: "+",
        args: [
          { kind: "const", value: 2, sort: Int },
          { kind: "const", value: 3, sort: Int },
        ],
        sort: Int,
      });
    });
  });

  describe("atomic predicates", () => {
    it("eq builds an atomic = formula", () => {
      const f = eq(num(0), num(0));
      expect(f).toEqual({
        kind: "atomic",
        predicate: "=",
        args: [
          { kind: "const", value: 0, sort: Int },
          { kind: "const", value: 0, sort: Int },
        ],
      });
    });

    it("gt builds an atomic > formula with mixed liftable args", () => {
      const f = gt(parseInt(str("0")), 0);
      if (f.kind === "atomic") {
        expect(f.predicate).toBe(">");
        expect(f.args).toHaveLength(2);
      }
    });
  });

  describe("property() + bridge() collection", () => {
    it("property() collects a PropertyDeclaration", () => {
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

    it("bridge() collects a BridgeDeclaration", () => {
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

    it("multiple property + bridge calls collect in order", () => {
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

    it("calling property without an active collector throws", () => {
      expect(() => property("x", eq(num(0), num(0)))).toThrow(/outside an active collector/);
    });

    it("nested beginCollecting throws", () => {
      const finish = beginCollecting();
      try {
        expect(() => beginCollecting()).toThrow(/already active/);
      } finally {
        finish();
      }
    });
  });

  describe("quantifiers", () => {
    it("forAll wraps a body builder", () => {
      const f = forAll(Int, (x) => gt(x, num(0)));
      expect(f.kind).toBe("forall");
      if (f.kind === "forall") {
        expect(f.sort).toEqual(Int);
        expect(f.predicate.body.kind).toBe("atomic");
      }
    });

    it("exists wraps a body builder", () => {
      const f = exists(StringSort, (s) => eq(parseInt(s), num(0)));
      expect(f.kind).toBe("exists");
    });

    it("nested quantifiers compose", () => {
      const f = forAll(Int, (x) =>
        forAll(Int, (y) => gt(add(x, y), num(0))),
      );
      expect(f.kind).toBe("forall");
    });
  });

  describe("describe() + it() ergonomics", () => {
    it("registers a single it() under a describe", async () => {
      const pk = await import("./index.js");
      const finish = beginCollecting();
      pk.describe("parseInt", () => {
        pk.it("canReturnZero",
          exists(StringSort, (s) => eq(parseInt(s), num(0))),
        );
      });
      const decls = finish();
      expect(decls).toHaveLength(1);
      expect(decls[0]!.name).toBe("parseInt > canReturnZero");
    });

    it("nested describes build a path", async () => {
      const pk = await import("./index.js");
      const finish = beginCollecting();
      pk.describe("Math", () => {
        pk.describe("abs", () => {
          pk.it("non-negative", forAll(Int, (x) => gt(abs(x), num(-1))));
        });
      });
      const decls = finish();
      expect(decls).toHaveLength(1);
      expect(decls[0]!.name).toBe("Math > abs > non-negative");
    });

    it("multiple it() in one describe", async () => {
      const pk = await import("./index.js");
      const finish = beginCollecting();
      pk.describe("parseInt", () => {
        pk.it("canReturnZero", eq(num(0), num(0)));
        pk.it("canReturnPositive", eq(num(1), num(1)));
      });
      const decls = finish();
      expect(decls).toHaveLength(2);
      expect(decls.map((d) => d.name)).toEqual([
        "parseInt > canReturnZero",
        "parseInt > canReturnPositive",
      ]);
    });

    it("describe pops its segment after body returns", async () => {
      const pk = await import("./index.js");
      const finish = beginCollecting();
      pk.describe("a", () => {
        pk.it("inner", eq(num(0), num(0)));
      });
      pk.it("outer", eq(num(1), num(1)));
      const decls = finish();
      expect(decls.map((d) => d.name)).toEqual(["a > inner", "outer"]);
    });
  });

  describe("the worked example: parseInt-can-return-zero", () => {
    it("composes via runtime evaluation, no AST walk needed", () => {
      const finish = beginCollecting();

      // This is the user's invariant code, written using symbolic primitives.
      // RUNNING this code produces the IR. No tsc Compiler API involved.
      property("parseIntCanReturnZero",
        exists(StringSort, (s) => eq(parseInt(s), num(0))),
      );

      const decls = finish();
      expect(decls).toHaveLength(1);
      const decl = decls[0]!;
      expect(decl.kind).toBe("property");
      if (decl.kind === "property") {
        expect(decl.name).toBe("parseIntCanReturnZero");
        expect(decl.formula.kind).toBe("exists");
        if (decl.formula.kind === "exists") {
          expect(decl.formula.sort).toEqual(StringSort);
        }
      }
    });
  });
});
