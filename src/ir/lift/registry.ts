/**
 * Pure-function registry. The lifter consults this when it encounters
 * a CallExpression to decide whether the call is admissible in the IR
 * subset. Calls not in the registry are diagnostic-rejected per spec
 * §6.2 "Side-effecting calls".
 *
 * Spec: docs/specs/2026-04-29-ts-ir-language.md §11
 *
 * v1 stores only sort signatures. Symbolic-range contracts (the
 * downstream prover input) are out of scope per cut list.
 */

import type { Sort } from "../formulas.js";

const Int: Sort = { kind: "primitive", name: "Int" };
const Real: Sort = { kind: "primitive", name: "Real" };
const Bool: Sort = { kind: "primitive", name: "Bool" };
const StringSort: Sort = { kind: "primitive", name: "String" };

export type RegistryReturnKind = "term" | "formula";

export interface RegistryEntry {
  name: string;
  signatureSorts: Sort[];
  returnSort: Sort;
  /**
   * "formula" if the call's return type is boolean and it should
   * lift to an atomic formula (e.g. `Number.isInteger(x)`).
   * "term" if the call returns a value (e.g. `Math.abs(x)`).
   * The lifter uses this to dispatch between liftFormula / liftTerm.
   */
  returnKind: RegistryReturnKind;
}

export interface PureFunctionRegistry {
  has(name: string): boolean;
  get(name: string): RegistryEntry | undefined;
  names(): string[];
}

class FrozenRegistry implements PureFunctionRegistry {
  private readonly entries: Map<string, RegistryEntry>;

  constructor(entries: Map<string, RegistryEntry>) {
    this.entries = entries;
  }

  has(name: string): boolean {
    return this.entries.has(name);
  }

  get(name: string): RegistryEntry | undefined {
    return this.entries.get(name);
  }

  names(): string[] {
    return Array.from(this.entries.keys());
  }
}

type EntrySpec = readonly [
  name: string,
  signatureSorts: Sort[],
  returnSort: Sort,
  returnKind: RegistryReturnKind,
];

const TS_KIT_BASELINE: readonly EntrySpec[] = [
  // Bare globals
  ["parseInt", [StringSort], Int, "term"],
  ["parseFloat", [StringSort], Real, "term"],
  ["isNaN", [Real], Bool, "formula"],
  ["isFinite", [Real], Bool, "formula"],
  ["String", [Int], StringSort, "term"],

  // Number static methods
  ["Number.isInteger", [Real], Bool, "formula"],
  ["Number.isFinite", [Real], Bool, "formula"],
  ["Number.isNaN", [Real], Bool, "formula"],
  ["Number.parseInt", [StringSort], Int, "term"],
  ["Number.parseFloat", [StringSort], Real, "term"],

  // Math
  ["Math.abs", [Real], Real, "term"],
  ["Math.max", [Real, Real], Real, "term"],
  ["Math.min", [Real, Real], Real, "term"],
  ["Math.floor", [Real], Real, "term"],
  ["Math.ceil", [Real], Real, "term"],
  ["Math.round", [Real], Real, "term"],
  ["Math.sign", [Real], Real, "term"],
  ["Math.sqrt", [Real], Real, "term"],
  ["Math.pow", [Real, Real], Real, "term"],
  ["Math.log", [Real], Real, "term"],
  ["Math.exp", [Real], Real, "term"],
  ["Math.sin", [Real], Real, "term"],
  ["Math.cos", [Real], Real, "term"],
  ["Math.tan", [Real], Real, "term"],

  // String prototype reads (modeled as named functions for v1)
  ["String.prototype.length", [StringSort], Int, "term"],
  ["String.prototype.charAt", [StringSort, Int], StringSort, "term"],
  ["String.prototype.charCodeAt", [StringSort, Int], Int, "term"],
  ["String.prototype.includes", [StringSort, StringSort], Bool, "formula"],
  ["String.prototype.startsWith", [StringSort, StringSort], Bool, "formula"],
  ["String.prototype.endsWith", [StringSort, StringSort], Bool, "formula"],

  // Array prototype reads
  ["Array.prototype.length", [{ kind: "primitive", name: "Ref" }], Int, "term"],
  ["Array.prototype.includes", [{ kind: "primitive", name: "Ref" }, Real], Bool, "formula"],
  ["Array.prototype.indexOf", [{ kind: "primitive", name: "Ref" }, Real], Int, "term"],
  ["Array.prototype.at", [{ kind: "primitive", name: "Ref" }, Int], Real, "term"],
];

export function defaultTsKitRegistry(): PureFunctionRegistry {
  const map = new Map<string, RegistryEntry>();
  for (const [name, signatureSorts, returnSort, returnKind] of TS_KIT_BASELINE) {
    map.set(name, { name, signatureSorts, returnSort, returnKind });
  }
  return new FrozenRegistry(map);
}

export function emptyRegistry(): PureFunctionRegistry {
  return new FrozenRegistry(new Map());
}

export function extendRegistry(
  base: PureFunctionRegistry,
  extras: RegistryEntry[],
): PureFunctionRegistry {
  const map = new Map<string, RegistryEntry>();
  for (const name of base.names()) {
    const e = base.get(name);
    if (e) map.set(name, e);
  }
  for (const e of extras) {
    map.set(e.name, e);
  }
  return new FrozenRegistry(map);
}
