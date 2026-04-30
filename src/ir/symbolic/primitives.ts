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
// Built-in function primitives — return IrTerm with kind="ctor"
//
// Each primitive represents a CALL to a kit-registered built-in function.
// The kit's catalog publishes axioms (in SMT-LIB) describing each.
// Consumers' invariants reference these primitives; running the invariant
// produces the IR; SMT solver uses the kit's axioms during verification.
// ---------------------------------------------------------------------------

function ctor(name: string, args: IrTerm[], sort: Sort): IrTerm {
  return { kind: "ctor", name, args, sort };
}

// Number parsing
export function parseInt(s: IrTerm): IrTerm {
  return ctor("parseInt", [s], Int);
}
export function parseFloat(s: IrTerm): IrTerm {
  return ctor("parseFloat", [s], Real);
}

// Number predicates as primitives (return Bool-typed terms; combine via eq if needed)
export function isNaN(n: IrTerm): IrTerm {
  return ctor("isNaN", [n], Bool);
}
export function isFinite(n: IrTerm): IrTerm {
  return ctor("isFinite", [n], Bool);
}
export function isInteger(n: IrTerm): IrTerm {
  return ctor("isInteger", [n], Bool);
}

// Math.* primitives — sort-correct for Int / Real
export function abs(n: IrTerm): IrTerm {
  return ctor("Math.abs", [n], n.sort ?? Real);
}
export function max(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("Math.max", [a, b], a.sort ?? Real);
}
export function min(a: IrTerm, b: IrTerm): IrTerm {
  return ctor("Math.min", [a, b], a.sort ?? Real);
}
export function floor(n: IrTerm): IrTerm {
  return ctor("Math.floor", [n], Int);
}
export function ceil(n: IrTerm): IrTerm {
  return ctor("Math.ceil", [n], Int);
}
export function sqrt(n: IrTerm): IrTerm {
  return ctor("Math.sqrt", [n], Real);
}
export function sign(n: IrTerm): IrTerm {
  return ctor("Math.sign", [n], Int);
}

// String.* primitives
export function stringLength(s: IrTerm): IrTerm {
  return ctor("String.prototype.length", [s], Int);
}
export function stringIncludes(s: IrTerm, sub: IrTerm): IrTerm {
  return ctor("String.prototype.includes", [s, sub], Bool);
}

// Array.* primitives (element type carried via the array's sort)
export function arrayLength(arr: IrTerm): IrTerm {
  return ctor("Array.prototype.length", [arr], Int);
}
export function arrayIncludes(arr: IrTerm, item: IrTerm): IrTerm {
  return ctor("Array.prototype.includes", [arr, item], Bool);
}

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
