/**
 * Contract and bridge collectors — symbolic primitives that capture IR
 * declarations into a thread-local collection.
 *
 * The user's invariant file imports `contract` / `must` / `bridge` from
 * this module. When the file runs (e.g.,
 * `await import("./parseInt.invariant.ts")`), each call adds an entry to
 * the active collector. The lifter then reads the collector's contents.
 *
 * No tsc compiler API. No AST walking. The user's code RUNS to produce IR.
 *
 * This is the meta-circular property of the framework: the IR comes from
 * RUNNING the invariant code, not from COMPILING it. Each kit's
 * symbolic primitives provide the IR-emission mechanism for that host
 * language.
 */

import type { IrFormula, IrTerm, Sort, VarTerm } from "../formulas.js";

// ---------------------------------------------------------------------------
// Declarations captured by the collector
// ---------------------------------------------------------------------------

export interface ContractDeclaration {
  kind: "contract";
  name: string;
  outBinding: string;
  pre?: IrFormula;
  post?: IrFormula;
  inv?: IrFormula;
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

export type Declaration = ContractDeclaration | BridgeDeclaration;

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
  contractStack = [];
  return () => {
    activeCollector = null;
    contractStack = [];
    return collector;
  };
}

/** Test helper: clear any active collector (use only in test setup/teardown). */
export function _resetCollector(): void {
  activeCollector = null;
  describePath = [];
  contractStack = [];
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
// ---------------------------------------------------------------------------

let describePath: string[] = [];

/**
 * Open a named describe block. The body runs immediately; any contract()
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

// ---------------------------------------------------------------------------
// contract() — primary primitive for declaring named contracts.
// ---------------------------------------------------------------------------

/**
 * Tracks the currently authoring contract so out() can resolve to the
 * right outBinding name. contract() may not be nested.
 */
let contractStack: Array<{ outBinding: string }> = [];

export interface ContractSpec {
  pre?: IrFormula;
  post?: IrFormula;
  inv?: IrFormula;
  outBinding?: string;
}

/**
 * Declare a named behavior contract. Carries any combination of
 * precondition, postcondition, and inductive invariant. At least one of
 * pre/post/inv MUST be provided. `outBinding` defaults to "out" and
 * names the variable the postcondition uses to reference the return
 * value (see out()).
 */
export function contract(name: string, spec: ContractSpec): void {
  if (activeCollector === null) {
    throw new Error(
      `contract("${name}", ...) called outside an active collector.`,
    );
  }
  if (spec.pre === undefined && spec.post === undefined && spec.inv === undefined) {
    throw new Error(
      `contract("${name}", ...) requires at least one of pre/post/inv.`,
    );
  }
  const outBinding = spec.outBinding ?? "out";
  contractStack.push({ outBinding });
  try {
    const fullName =
      describePath.length === 0 ? name : `${describePath.join(" > ")} > ${name}`;
    const decl: ContractDeclaration = {
      kind: "contract",
      name: fullName,
      outBinding,
    };
    if (spec.pre !== undefined) decl.pre = spec.pre;
    if (spec.post !== undefined) decl.post = spec.post;
    if (spec.inv !== undefined) decl.inv = spec.inv;
    activeCollector.push(decl);
  } finally {
    contractStack.pop();
  }
}

/**
 * Convenience alias for the precondition-only case:
 *   must(name, pre)  ===  contract(name, { pre })
 *
 * The verb `must` is non-negotiable: invariants are obligations, not
 * observations. The framework writes invariants in the obligation
 * register; the API forces that register at call sites.
 */
export function must(name: string, pre: IrFormula): void {
  contract(name, { pre });
}

/** Skip an invariant. The declaration is acknowledged but not collected. */
must.skip = function (name: string, _pre: IrFormula): void {
  void name;
};

/** Skip a describe block. */
describe.skip = function (name: string, _body: () => void): void {
  void name;
};

// ---------------------------------------------------------------------------
// out() — references the function's return value within a `post` formula.
//
// Compiles to a VarTerm whose `name` matches the enclosing contract's
// outBinding (default "out"). Outside a contract() call, out() defaults
// to "out" so post-only fragments built before contract() composes work.
// ---------------------------------------------------------------------------

export function out(): VarTerm {
  const top = contractStack[contractStack.length - 1];
  return { kind: "var", name: top?.outBinding ?? "out" };
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
