/**
 * Scope helpers: builders for BindingScope values used in property declarations.
 */

import type { BindingScope, IrFormula } from "./formulas.js";

/** Scope to a named function. */
export function function_(name: string): BindingScope {
  return { kind: "function", name };
}

/** Scope to a module (by path or module name). */
export function module_(path: string): BindingScope {
  return { kind: "module", path };
}

/** Scope to a class definition. */
export function class_(name: string): BindingScope {
  return { kind: "class", name };
}

/** Scope to a method on a class. */
export function method_(className: string, methodName: string): BindingScope {
  return { kind: "method", className, methodName };
}

/** Scope to a code region delimited by start and end markers. */
export function region(start: string, end: string): BindingScope {
  return { kind: "region", start, end };
}

/** Scope to a state transition. */
export function transition(name: string): BindingScope {
  return { kind: "transition", name };
}

/**
 * Scope to all sites where `predicate` holds.
 * The predicate is an IrFormula that can reference program points.
 */
export function whenever(predicate: IrFormula): BindingScope {
  return { kind: "whenever", predicate };
}
