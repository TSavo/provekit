/**
 * @provekit/ir/symbolic — runtime-eval lifting via symbolic primitives.
 *
 * The kit's IR-emission mechanism. Users import primitives from this
 * module, write invariant code with them, and RUNNING the code produces
 * the IR. No tsc compiler API. No AST walking. Just function calls.
 *
 * This is the tagless-final / free-monad pattern applied to ProvekIt.
 * Each primitive returns an IR data structure when called; the user's
 * predicate function's return value IS the IrFormula.
 *
 * Usage:
 *   import {
 *     contract, must, out, bridge, forAll, exists, eq, parseInt, num, Int
 *   } from "@provekit/ir/symbolic";
 *
 *   contract("zeroIsZero", {
 *     pre: eq(parseInt(num("0")), num(0)),
 *   });
 *
 *   bridge("parseIntBridgesV8", {
 *     sourceSymbol: "global.parseInt",
 *     sourceLayer: "ts-kit@1.0",
 *     targetContractCid: "abc...",
 *     targetLayer: "V8@12.4",
 *   });
 *
 * To lift: call beginCollecting(), await import("user.invariant.ts"),
 * then call the returned finalizer to retrieve all collected declarations.
 */

// Sorts (from the existing IR library)
export {
  Bool,
  Int,
  Real,
  String,
  Ref,
  Node,
  Edge,
  SetOf,
  TupleOf,
  FuncOf,
  BV,
  BV8,
  BV16,
  BV32,
  BV64,
  BV128,
  BV256,
} from "../sorts.js";
export type { IrFormula, IrTerm, Sort } from "../formulas.js";

// Contract + bridge collection
export {
  contract,
  bridge,
  describe,
  must,
  out,
  beginCollecting,
  _resetCollector,
  forAll,
  exists,
  and,
  or,
  not,
  implies,
  iff,
} from "./property.js";
export type {
  Declaration,
  ContractDeclaration,
  BridgeDeclaration,
  BridgeSpec,
  ContractSpec,
} from "./property.js";

// Constants
export { num, real, str, bool } from "./primitives.js";

// Built-in function primitives
export {
  parseInt,
  parseFloat,
  isNaN,
  isFinite,
  isInteger,
  abs,
  max,
  min,
  floor,
  ceil,
  sqrt,
  sign,
  stringLength,
  stringIncludes,
  arrayLength,
  arrayIncludes,
} from "./primitives.js";

// Term-level arithmetic
export { add, sub, mul, div, neg } from "./primitives.js";

// Atomic predicates
export { eq, neq, lt, lte, gt, gte, isTrue, isFalse } from "./primitives.js";

// Bitvector primitives (SMT-LIB BV theory)
export {
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
} from "./primitives.js";
