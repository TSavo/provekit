/**
 * Primitive bridge authoring tests.
 *
 * The kit's "built-in" primitives that aren't actually kit-owned
 * (parseInt, abs, floor, etc.: owned by V8/ECMA-262) get authored
 * as bridges, not extensions. These tests verify the bridge factory
 * registers declarations correctly and emits IR ctor nodes the user
 * can call directly.
 */

import { describe, it, expect, beforeEach } from "vitest";
import {
  primitiveBridge,
  listBridges,
  lookupBridge,
  _resetBridges,
} from "./bridges.js";
import { num, str } from "../symbolic/primitives.js";
import { Int, String as StringSort } from "../sorts.js";

beforeEach(() => {
  _resetBridges();
});

describe("primitiveBridge", () => {
  it("returns a function that builds ctor IrTerms with the bridged name", () => {
    const parseInt = primitiveBridge({
      irName: "parseInt",
      irArgSorts: [StringSort],
      irReturnSort: Int,
      sourceLayer: "ts-kit",
      targetContractCid: "bafy_V8_PARSEINT_PLACEHOLDER",
      targetLayer: "v8",
      notes: "Bridges to V8's parseInt implementation per ECMA-262.",
    });
    const term = parseInt(str("42"));
    expect(term.kind).toBe("ctor");
    expect((term as { name: string }).name).toBe("parseInt");
    // Spec v1.1: ctor terms carry no `sort` field on the wire; the
    // factory tracks return sort via a non-enumerable side channel.
    const sortHint = (term as unknown as Record<symbol, unknown>)[
      Symbol.for("provekit.ir.sortHint")
    ];
    expect(sortHint).toEqual(Int);
  });

  it("registers the bridge declaration in the registry", () => {
    primitiveBridge({
      irName: "parseInt",
      irArgSorts: [StringSort],
      irReturnSort: Int,
      sourceLayer: "ts-kit",
      targetContractCid: "bafy_V8_PARSEINT",
      targetLayer: "v8",
    });
    const decl = lookupBridge("parseInt");
    expect(decl).not.toBeNull();
    expect(decl!.targetContractCid).toBe("bafy_V8_PARSEINT");
    expect(decl!.targetLayer).toBe("v8");
  });

  it("is idempotent for byte-identical re-registration", () => {
    const input = {
      irName: "parseInt",
      irArgSorts: [StringSort],
      irReturnSort: Int,
      sourceLayer: "ts-kit",
      targetContractCid: "bafy_V8_PARSEINT",
      targetLayer: "v8",
    };
    primitiveBridge(input);
    expect(() => primitiveBridge(input)).not.toThrow();
  });

  it("throws on collision with a different target", () => {
    primitiveBridge({
      irName: "parseInt",
      irArgSorts: [StringSort],
      irReturnSort: Int,
      sourceLayer: "ts-kit",
      targetContractCid: "bafy_V8_PARSEINT",
      targetLayer: "v8",
    });
    expect(() =>
      primitiveBridge({
        irName: "parseInt",
        irArgSorts: [StringSort],
        irReturnSort: Int,
        sourceLayer: "ts-kit",
        targetContractCid: "bafy_DIFFERENT",
        targetLayer: "node",
      }),
    ).toThrow(/already registered with a different target/);
  });

  it("listBridges returns all registered bridges", () => {
    primitiveBridge({
      irName: "parseInt",
      irArgSorts: [StringSort],
      irReturnSort: Int,
      sourceLayer: "ts-kit",
      targetContractCid: "cid1",
      targetLayer: "v8",
    });
    primitiveBridge({
      irName: "abs",
      irArgSorts: [Int],
      irReturnSort: Int,
      sourceLayer: "ts-kit",
      targetContractCid: "cid2",
      targetLayer: "v8",
    });
    expect(listBridges()).toHaveLength(2);
  });

  it("dogfood: a small set of V8-bridged primitives all register and emit IR", () => {
    const parseInt = primitiveBridge({
      irName: "parseInt",
      irArgSorts: [StringSort],
      irReturnSort: Int,
      sourceLayer: "ts-kit",
      targetContractCid: "bafy_V8_PARSEINT",
      targetLayer: "v8",
    });
    const abs = primitiveBridge({
      irName: "abs",
      irArgSorts: [Int],
      irReturnSort: Int,
      sourceLayer: "ts-kit",
      targetContractCid: "bafy_V8_MATH_ABS",
      targetLayer: "v8",
    });
    const floor = primitiveBridge({
      irName: "floor",
      irArgSorts: [Int],
      irReturnSort: Int,
      sourceLayer: "ts-kit",
      targetContractCid: "bafy_V8_MATH_FLOOR",
      targetLayer: "v8",
    });

    // Use them in IR formulas
    const t1 = parseInt(str("123"));
    const t2 = abs(num(-5));
    const t3 = floor(num(7));

    expect(t1).toMatchObject({ kind: "ctor", name: "parseInt" });
    expect(t2).toMatchObject({ kind: "ctor", name: "abs" });
    expect(t3).toMatchObject({ kind: "ctor", name: "floor" });

    // All three bridges are in the registry, all targeting V8
    expect(listBridges()).toHaveLength(3);
    for (const b of listBridges()) {
      expect(b.targetLayer).toBe("v8");
    }
  });
});
