/**
 * Symbolic primitives — IR builders for kit-supplied built-in functions.
 *
 * The runtime-eval lifting model: instead of walking the user's TypeScript
 * AST via the tsc Compiler API, the user imports symbolic primitives that
 * BUILD IR NODES when called. Running the user's invariant function
 * produces the IR directly. The function's return value IS the IrFormula.
 *
 * This is the tagless-final / free-monad pattern. Each primitive returns
 * an IrTerm or IrFormula data structure; nothing actually computes
 * parseInt's value or compares numbers. The "evaluation" is symbolic
 * construction, not concrete computation.
 *
 * Example:
 *   import { property, forAll, parseInt, eq, num, Int } from "provekit/ir/symbolic";
 *
 *   property("zeroIsZero",
 *     eq(parseInt(num("0")), num(0))
 *   );
 *
 * Running this file runs the property() call. property() collects the
 * IR. The lifter just imports the file and reads what was collected.
 * No tsc compiler API. No AST walking. Just function calls.
 */

import type { IrFormula, IrTerm, Sort } from "../formulas.js";
import { liftToTerm } from "../formulas.js";
import { Int, Real, Bool, String as StringSort } from "../sorts.js";

// ---------------------------------------------------------------------------
// Type-friendly aliases for the user's surface API
// ---------------------------------------------------------------------------

export type Term = IrTerm;
export type Formula = IrFormula;

// ---------------------------------------------------------------------------
// Constants — `num`, `str`, `bool`
// ---------------------------------------------------------------------------

/** Build an Int constant term. */
export function num(value: number | bigint): IrTerm {
  if (typeof value === "bigint" || (typeof value === "number" && Number.isInteger(value))) {
    return { kind: "const", value, sort: Int };
  }
  return { kind: "const", value, sort: Real };
}

/** Build a Real constant term. */
export function real(value: number): IrTerm {
  return { kind: "const", value, sort: Real };
}

/** Build a String constant term. */
export function str(value: string): IrTerm {
  return { kind: "const", value, sort: StringSort };
}

/** Build a Bool constant term. */
export function bool(value: boolean): IrTerm {
  return { kind: "const", value, sort: Bool };
}

// ---------------------------------------------------------------------------
// Built-in function primitives.
//
// Most of these are NOT owned by the TS kit. Their semantic authority
// lives in V8 / ECMA-262 / IEEE 754. The kit doesn't load V8 or
// re-implement parseInt; it BRIDGES to V8's signed claims via a CID,
// using the primitiveBridge factory. The user's API is unchanged
// (`parseInt(s)` still returns an IrTerm); the kit's claim about
// what parseInt MEANS is now explicit: a bridge to the deeper layer.
//
// At module load, each primitive registers a bridge declaration in
// the kit's registry. Verifiers walk the registry to resolve IR
// names through the protocol's resolver semantics
// (protocol/specs/2026-04-30-ir-extension-protocol.md §5).
//
// `targetContractCid` values here are placeholders today. When the
// V8 / ECMA-262 catalogs are published with signed declarations,
// these CIDs get pinned to specific signed mementos.
// ---------------------------------------------------------------------------

import { primitiveBridge } from "../extensions/bridges.js";

const TS_KIT = "ts-kit";
const V8 = "v8";
const ECMA262 = "ecma-262";

function ctor(name: string, args: IrTerm[], sort: Sort): IrTerm {
  return { kind: "ctor", name, args, sort };
}

// Number parsing — bridged to V8's ECMA-262 implementation.
export const parseInt = primitiveBridge({
  irName: "parseInt",
  irArgSorts: [StringSort],
  irReturnSort: Int,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_PARSEINT_PLACEHOLDER",
  targetLayer: V8,
  notes: "ECMA-262 parseInt; bridged to V8's signed declaration.",
});

export const parseFloat = primitiveBridge({
  irName: "parseFloat",
  irArgSorts: [StringSort],
  irReturnSort: Real,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_PARSEFLOAT_PLACEHOLDER",
  targetLayer: V8,
  notes: "ECMA-262 parseFloat.",
});

// Number predicates — bridged.
export const isNaN = primitiveBridge({
  irName: "isNaN",
  irArgSorts: [Real],
  irReturnSort: Bool,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_ISNAN_PLACEHOLDER",
  targetLayer: V8,
});

export const isFinite = primitiveBridge({
  irName: "isFinite",
  irArgSorts: [Real],
  irReturnSort: Bool,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_ISFINITE_PLACEHOLDER",
  targetLayer: V8,
});

export const isInteger = primitiveBridge({
  irName: "isInteger",
  irArgSorts: [Real],
  irReturnSort: Bool,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_ISINTEGER_PLACEHOLDER",
  targetLayer: V8,
});

// Math.* polymorphic primitives — return sort mirrors operand sort.
// The simple primitiveBridge factory captures a fixed return sort, so
// abs/max/min stay as raw ctor calls for now. A future per-sort split
// (Math.abs.int / Math.abs.real) would let them bridge cleanly. TODO.
export function abs(n: IrTerm): IrTerm {
  return ctor("Math.abs", [n], n.sort ?? Real);
}
export function max(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("Math.max", [a, b], a.sort ?? Real);
}
export function min(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("Math.min", [a, b], a.sort ?? Real);
}

// Math.* monomorphic primitives — bridged.
export const floor = primitiveBridge({
  irName: "Math.floor",
  irArgSorts: [Real],
  irReturnSort: Int,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_MATH_FLOOR_PLACEHOLDER",
  targetLayer: V8,
});

export const ceil = primitiveBridge({
  irName: "Math.ceil",
  irArgSorts: [Real],
  irReturnSort: Int,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_MATH_CEIL_PLACEHOLDER",
  targetLayer: V8,
});

export const sqrt = primitiveBridge({
  irName: "Math.sqrt",
  irArgSorts: [Real],
  irReturnSort: Real,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_MATH_SQRT_PLACEHOLDER",
  targetLayer: V8,
});

export const sign = primitiveBridge({
  irName: "Math.sign",
  irArgSorts: [Real],
  irReturnSort: Int,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_MATH_SIGN_PLACEHOLDER",
  targetLayer: V8,
});

// String.* primitives — bridged.
export const stringLength = primitiveBridge({
  irName: "String.prototype.length",
  irArgSorts: [StringSort],
  irReturnSort: Int,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_STRING_LENGTH_PLACEHOLDER",
  targetLayer: V8,
});

export const stringIncludes = primitiveBridge({
  irName: "String.prototype.includes",
  irArgSorts: [StringSort, StringSort],
  irReturnSort: Bool,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_STRING_INCLUDES_PLACEHOLDER",
  targetLayer: V8,
});

// Array.* primitives — bridged. Element type carried by the array's sort.
export const arrayLength = primitiveBridge({
  irName: "Array.prototype.length",
  irArgSorts: ["Array"],
  irReturnSort: Int,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_ARRAY_LENGTH_PLACEHOLDER",
  targetLayer: V8,
});

export const arrayIncludes = primitiveBridge({
  irName: "Array.prototype.includes",
  irArgSorts: ["Array", "Any"],
  irReturnSort: Bool,
  sourceLayer: TS_KIT,
  targetContractCid: "bafy_V8_ARRAY_INCLUDES_PLACEHOLDER",
  targetLayer: V8,
});

// ---------------------------------------------------------------------------
// Term-level arithmetic — return IrTerm
// ---------------------------------------------------------------------------

export function add(a: IrTerm | number, b: IrTerm | number): IrTerm {
  return ctor("+", [liftToTerm(a), liftToTerm(b)], Int);
}
export function sub(a: IrTerm | number, b: IrTerm | number): IrTerm {
  return ctor("-", [liftToTerm(a), liftToTerm(b)], Int);
}
export function mul(a: IrTerm | number, b: IrTerm | number): IrTerm {
  return ctor("*", [liftToTerm(a), liftToTerm(b)], Int);
}
export function div(a: IrTerm | number, b: IrTerm | number): IrTerm {
  return ctor("/", [liftToTerm(a), liftToTerm(b)], Real);
}
export function neg(a: IrTerm | number): IrTerm {
  return ctor("-", [liftToTerm(a)], Int);
}

// ---------------------------------------------------------------------------
// Atomic predicates — return IrFormula
// ---------------------------------------------------------------------------

type Liftable = IrTerm | number | bigint | string | boolean | null;

function atom(predicate: string, args: Liftable[]): IrFormula {
  return { kind: "atomic", predicate, args: args.map(liftToTerm) };
}

export function eq(a: Liftable, b: Liftable): IrFormula {
  return atom("=", [a, b]);
}
export function neq(a: Liftable, b: Liftable): IrFormula {
  return atom("≠", [a, b]);
}
export function lt(a: Liftable, b: Liftable): IrFormula {
  return atom("<", [a, b]);
}
export function lte(a: Liftable, b: Liftable): IrFormula {
  return atom("≤", [a, b]);
}
export function gt(a: Liftable, b: Liftable): IrFormula {
  return atom(">", [a, b]);
}
export function gte(a: Liftable, b: Liftable): IrFormula {
  return atom("≥", [a, b]);
}

/** Truthiness of a Bool-typed term. */
export function isTrue(b: Liftable): IrFormula {
  return atom("true", [b]);
}
/** Falsiness of a Bool-typed term. */
export function isFalse(b: Liftable): IrFormula {
  return atom("false", [b]);
}

// ---------------------------------------------------------------------------
// Bitvector primitives (SMT-LIB BV theory).
//
// `bv(value, width)` constructs a BV<width> constant. The value is
// normalized to the unsigned representation (modulo 2^width), so
// `bv(-1n, 8)` and `bv(255n, 8)` produce identical IR. This matches the
// SMT-LIB convention where bitvector literals are unsigned bit patterns.
//
// Term ctors (`bvadd`, `bvxor`, etc.) preserve the operand width as the
// result sort; binary ops require operand widths to match. `concat`
// builds a wider BV from two; `extract(hi, lo, x)` takes a slice and
// produces a BV<hi-lo+1>. Comparison predicates (`bvult`, `bvslt`, ...)
// return IrFormula.
// ---------------------------------------------------------------------------

function bvSortOf(t: IrTerm): { kind: "bitvec"; width: number } {
  const s = t.sort;
  if (s.kind !== "bitvec") {
    throw new Error(`bv* primitive: expected a BV-sorted term, got sort kind "${s.kind}"`);
  }
  return s;
}

function requireSameWidth(a: IrTerm, b: IrTerm, op: string): { kind: "bitvec"; width: number } {
  const sa = bvSortOf(a);
  const sb = bvSortOf(b);
  if (sa.width !== sb.width) {
    throw new Error(
      `${op}: operand widths must match (got ${sa.width} and ${sb.width})`,
    );
  }
  return sa;
}

/** Build a BV constant of the given width. Value is normalized to unsigned mod 2^width. */
export function bv(value: number | bigint, width: number): IrTerm {
  if (!Number.isInteger(width) || width <= 0) {
    throw new Error(`bv: width must be a positive integer, got ${width}`);
  }
  const big = typeof value === "bigint" ? value : BigInt(value);
  const modulus = 1n << BigInt(width);
  let normalized = big % modulus;
  if (normalized < 0n) normalized += modulus;
  return { kind: "const", value: normalized, sort: { kind: "bitvec", width } };
}

// Binary BV term ctors — return BV<w> where w matches both operands.

export function bvadd(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("bvadd", [a, b], requireSameWidth(a, b, "bvadd"));
}
export function bvsub(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("bvsub", [a, b], requireSameWidth(a, b, "bvsub"));
}
export function bvmul(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("bvmul", [a, b], requireSameWidth(a, b, "bvmul"));
}
export function bvudiv(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("bvudiv", [a, b], requireSameWidth(a, b, "bvudiv"));
}
export function bvurem(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("bvurem", [a, b], requireSameWidth(a, b, "bvurem"));
}
export function bvshl(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("bvshl", [a, b], requireSameWidth(a, b, "bvshl"));
}
export function bvlshr(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("bvlshr", [a, b], requireSameWidth(a, b, "bvlshr"));
}
export function bvashr(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("bvashr", [a, b], requireSameWidth(a, b, "bvashr"));
}
export function bvor(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("bvor", [a, b], requireSameWidth(a, b, "bvor"));
}
export function bvand(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("bvand", [a, b], requireSameWidth(a, b, "bvand"));
}
export function bvxor(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("bvxor", [a, b], requireSameWidth(a, b, "bvxor"));
}

// Unary BV term ctors.

export function bvnot(a: IrTerm): IrTerm {
  return ctor("bvnot", [a], bvSortOf(a));
}
export function bvneg(a: IrTerm): IrTerm {
  return ctor("bvneg", [a], bvSortOf(a));
}

// concat: BV<a> × BV<b> -> BV<a+b>. SMT-LIB ordering: high bits come from
// the first operand.
export function concat(a: IrTerm, b: IrTerm): IrTerm {
  const sa = bvSortOf(a);
  const sb = bvSortOf(b);
  return ctor("concat", [a, b], { kind: "bitvec", width: sa.width + sb.width });
}

// extract(hi, lo, x): slice bits [hi:lo] inclusive, producing BV<hi-lo+1>.
// hi and lo are encoded as Int constants in the IR; the SMT translator
// special-cases them as the indexed (_ extract hi lo) operator.
export function extract(hi: number, lo: number, x: IrTerm): IrTerm {
  const sx = bvSortOf(x);
  if (!Number.isInteger(hi) || !Number.isInteger(lo)) {
    throw new Error(`extract: hi and lo must be integers, got hi=${hi} lo=${lo}`);
  }
  if (hi < lo || lo < 0 || hi >= sx.width) {
    throw new Error(
      `extract: indices out of range for BV${sx.width} (hi=${hi}, lo=${lo})`,
    );
  }
  const hiTerm: IrTerm = { kind: "const", value: BigInt(hi), sort: Int };
  const loTerm: IrTerm = { kind: "const", value: BigInt(lo), sort: Int };
  return ctor("extract", [hiTerm, loTerm, x], { kind: "bitvec", width: hi - lo + 1 });
}

// BV comparison predicates — return IrFormula.

function bvCmp(predicate: string, a: IrTerm, b: IrTerm): IrFormula {
  requireSameWidth(a, b, predicate);
  return { kind: "atomic", predicate, args: [a, b] };
}

export function bvult(a: IrTerm, b: IrTerm): IrFormula { return bvCmp("bvult", a, b); }
export function bvule(a: IrTerm, b: IrTerm): IrFormula { return bvCmp("bvule", a, b); }
export function bvugt(a: IrTerm, b: IrTerm): IrFormula { return bvCmp("bvugt", a, b); }
export function bvuge(a: IrTerm, b: IrTerm): IrFormula { return bvCmp("bvuge", a, b); }
export function bvslt(a: IrTerm, b: IrTerm): IrFormula { return bvCmp("bvslt", a, b); }
export function bvsle(a: IrTerm, b: IrTerm): IrFormula { return bvCmp("bvsle", a, b); }
export function bvsgt(a: IrTerm, b: IrTerm): IrFormula { return bvCmp("bvsgt", a, b); }
export function bvsge(a: IrTerm, b: IrTerm): IrFormula { return bvCmp("bvsge", a, b); }
