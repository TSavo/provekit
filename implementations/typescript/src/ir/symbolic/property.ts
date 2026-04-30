/**
 * Property and bridge collectors — symbolic primitives that capture IR
 * declarations into a thread-local collection.
 *
 * The user's invariant file imports `property` and `bridge` from this
 * module. When the file runs (e.g., `await import("./parseInt.invariant.ts")`),
 * each call to `property()` and `bridge()` adds an entry to the active
 * collector. The lifter then reads the collector's contents.
 *
 * No tsc compiler API. No AST walking. The user's code RUNS to produce IR.
 *
 * This is the meta-circular property of the framework: the IR comes from
 * RUNNING the invariant code, not from COMPILING it. Each kit's
 * symbolic primitives provide the IR-emission mechanism for that host
 * language.
 */

import type { IrFormula, IrTerm, Sort } from "../formulas.js";

// ---------------------------------------------------------------------------
// Declarations captured by the collector
// ---------------------------------------------------------------------------

export interface PropertyDeclaration {
  kind: "property";
  name: string;
  formula: IrFormula;
}

export interface BridgeDeclaration {
  kind: "bridge";
  name: string;
  sourceSymbol: string;
  sourceLayer: string;
  targetContractCid: string;
  targetLayer: string;
  notes?: string;
}

export type Declaration = PropertyDeclaration | BridgeDeclaration;

// ---------------------------------------------------------------------------
// Active collector — module-scoped, set by the lifter before importing
// the user's invariant file.
// ---------------------------------------------------------------------------

let activeCollector: Declaration[] | null = null;

/**
 * Begin collecting declarations. Returns a function that finalizes
 * collection and returns the captured declarations.
 *
 * Usage (lifter):
 *   const finish = beginCollecting();
 *   await import("./user-invariants.invariant.ts");
 *   const declarations = finish();
 */
export function beginCollecting(): () => Declaration[] {
  if (activeCollector !== null) {
    throw new Error(
      "beginCollecting: another collection is already active; lifting is not re-entrant",
    );
  }
  const collector: Declaration[] = [];
  activeCollector = collector;
  return () => {
    activeCollector = null;
    return collector;
  };
}

/** Test helper: clear any active collector (use only in test setup/teardown). */
export function _resetCollector(): void {
  activeCollector = null;
  describePath = [];
  // Reset the quantifier variable counter so successive runs of the
  // same invariant code produce identical IR (and therefore identical
  // CIDs). The counter generates fresh variable names like `_x0`, `_x1`;
  // without resetting it across runs, the second run gets `_x2`, `_x3`,
  // and the evidence-body raw IR differs even though the canonical FOL
  // is the same. The propertyHash (canonical, de-Bruijn-indexed) is
  // unaffected, but the envelope CID hashes the raw evidence body.
  _resetQuantifierCounter();
}

// ---------------------------------------------------------------------------
// describe() / it() — nested-context property declaration sugar.
//
// Mirrors vitest / jest / mocha. Authors organize invariants in nested
// describe blocks; it() declares a single named property whose full
// name is the path through the describe tree.
//
//   describe("parseInt", () => {
//     it("canReturnZero",
//       exists(StringSort, s => eq(parseInt(s), num(0)))
//     );
//
//     describe("non-negative integers", () => {
//       it("round-trip",
//         forAll(Int, n =>
//           implies(gte(n, num(0)), eq(parseInt(toString(n)), n))
//         )
//       );
//     });
//   });
//
// Yields property names: "parseInt > canReturnZero" and
// "parseInt > non-negative integers > round-trip".
// ---------------------------------------------------------------------------

let describePath: string[] = [];

/**
 * Open a named describe block. The body runs immediately; any property()
 * or it() calls inside register with the describe path prepended.
 *
 * Nesting is supported. The path uses " > " as a separator.
 */
export function describe(name: string, body: () => void): void {
  describePath.push(name);
  try {
    body();
  } finally {
    describePath.pop();
  }
}

/**
 * Declare a named invariant. The active describe path is used as a
 * prefix for the full name.
 *
 * The verb `must` is non-negotiable: invariants are obligations, not
 * observations. `it("returns zero")` reads as a test ("it does this");
 * `must("never throw on empty input")` reads as a constraint ("this
 * is required"). The framework writes invariants in the obligation
 * register; the API forces that register at call sites.
 */
export function must(name: string, formula: IrFormula): void {
  const fullName =
    describePath.length === 0 ? name : `${describePath.join(" > ")} > ${name}`;
  property(fullName, formula);
}

/** Skip an invariant. The declaration is acknowledged but not collected. */
must.skip = function (name: string, _formula: IrFormula): void {
  void name;
};

/** Skip a describe block. */
describe.skip = function (name: string, _body: () => void): void {
  void name;
};

// ---------------------------------------------------------------------------
// property() — declares a named property whose body is an IrFormula
// ---------------------------------------------------------------------------

/**
 * Declare a named property with an IR formula body.
 *
 * @param name — the property's identifier (used for diagnostics + memento naming)
 * @param formula — the IR formula constructed via the kit's symbolic primitives
 */
export function property(name: string, formula: IrFormula): void {
  if (activeCollector === null) {
    throw new Error(
      `property("${name}", ...) called outside an active collector. ` +
      `If you're running this file standalone for testing, call beginCollecting() first.`,
    );
  }
  activeCollector.push({ kind: "property", name, formula });
}

// ---------------------------------------------------------------------------
// bridge() — declares that a host-language symbol bridges to a deeper-layer
// published contract by CID.
// ---------------------------------------------------------------------------

export interface BridgeSpec {
  sourceSymbol: string;
  sourceLayer: string;
  targetContractCid: string;
  targetLayer: string;
  notes?: string;
}

/**
 * Declare a bridge from a host-language symbol to a deeper-layer contract.
 *
 * @param name — the bridge's identifier
 * @param spec — the bridge specification (source, target, optional notes)
 */
export function bridge(name: string, spec: BridgeSpec): void {
  if (activeCollector === null) {
    throw new Error(
      `bridge("${name}", ...) called outside an active collector.`,
    );
  }
  activeCollector.push({
    kind: "bridge",
    name,
    sourceSymbol: spec.sourceSymbol,
    sourceLayer: spec.sourceLayer,
    targetContractCid: spec.targetContractCid,
    targetLayer: spec.targetLayer,
    ...(spec.notes !== undefined ? { notes: spec.notes } : {}),
  });
}

// ---------------------------------------------------------------------------
// Quantifier wrappers — exported here so the symbolic module is the
// single import point. (These delegate to the existing IR library's
// builders.)
// ---------------------------------------------------------------------------

import {
  forAll as _forAll,
  exists as _exists,
  _resetCounter as _resetQuantifierCounter,
} from "../quantifiers.js";

export function forAll(sort: Sort, body: (x: IrTerm) => IrFormula): IrFormula {
  return _forAll(sort, body);
}

export function exists(sort: Sort, body: (x: IrTerm) => IrFormula): IrFormula {
  return _exists(sort, body);
}

// ---------------------------------------------------------------------------
// Connectives
// ---------------------------------------------------------------------------

import {
  and as _and,
  or as _or,
  not as _not,
  implies as _implies,
  iff as _iff,
} from "../connectives.js";

export const and = _and;
export const or = _or;
export const not = _not;
export const implies = _implies;
export const iff = _iff;
