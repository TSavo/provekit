/**
 * Primitive bridge authoring — kit "built-ins" that aren't actually
 * kit-owned.
 *
 * Most TS-kit "built-in" primitives (parseInt, abs, floor, ceil,
 * sqrt, max, min, isNaN, isFinite, stringLength, stringIncludes,
 * arrayLength, ...) aren't owned by the TS kit. Their semantic
 * authority lives in V8 / ECMA-262 / IEEE 754 / silicon. The kit
 * doesn't load V8 or re-implement parseInt; it BRIDGES to V8's
 * signed claims about parseInt's semantics.
 *
 * `primitiveBridge` is the factory for declaring such bridges. It:
 *   1. Returns a function the user calls in IR formulas (e.g.
 *      `parseInt(s)` returns an IrTerm).
 *   2. Records a bridge declaration that the verifier consults to
 *      resolve the IR name through to the deeper layer's authority.
 *
 * The verifier's chain: TS-kit's `parseInt` IR → bridge to V8's
 * parseInt declaration → V8's signed catalog → V8's release-team
 * signature → trust V8 (per the consumer's policy on which deeper
 * layers' signatures are accepted).
 *
 * Per the memento envelope grammar's bridge-memento variant. This
 * factory captures the bridge metadata at kit-authoring time;
 * production deployments persist the bridge memento to the local
 * memento store with a real signature when published.
 *
 * Compare: extensionCtor (kit OWNS the semantics) vs primitiveBridge
 * (kit references a deeper layer's authority). Use primitiveBridge
 * for anything where the IR name's meaning is determined by a layer
 * deeper than the kit (V8, kernel, hardware).
 */

import type { IrTerm, Sort } from "../formulas.js";
import { liftToTerm } from "../formulas.js";
import type { SortRef } from "./registry.js";

// ---------------------------------------------------------------------------
// Bridge declaration record
// ---------------------------------------------------------------------------

export interface PrimitiveBridgeDeclaration {
  /** The IR ctor name that appears in IR formulas. */
  irName: string;
  /** The IR-side argument sorts (each a SortRef). */
  irArgSorts: SortRef[];
  /** The IR-side return sort. */
  irReturnSort: SortRef;
  /** The kit's identifying layer name (e.g. "ts-kit", "rust-kit"). */
  sourceLayer: string;
  /**
   * CID of the deeper-layer's declaration this bridges to (e.g. V8's
   * signed memento for parseInt). At authoring time this may be a
   * placeholder until the deeper-layer's catalog is published.
   */
  targetContractCid: string;
  /** Deeper layer's name (e.g. "v8", "ecma-262", "node-runtime"). */
  targetLayer: string;
  /** Optional human-readable note explaining the bridge. */
  notes?: string;
}

// ---------------------------------------------------------------------------
// Bridge registry — process-local mirror of registered bridges
// ---------------------------------------------------------------------------

interface BridgeRegistryState {
  byIrName: Map<string, PrimitiveBridgeDeclaration>;
}

let state: BridgeRegistryState = { byIrName: new Map() };

/** List every registered primitive bridge. */
export function listBridges(): PrimitiveBridgeDeclaration[] {
  return [...state.byIrName.values()];
}

/** Look up a bridge by IR name. */
export function lookupBridge(irName: string): PrimitiveBridgeDeclaration | null {
  return state.byIrName.get(irName) ?? null;
}

/** Reset the bridge registry. Tests call this between cases. */
export function _resetBridges(): void {
  state = { byIrName: new Map() };
}

// ---------------------------------------------------------------------------
// primitiveBridge — the factory
// ---------------------------------------------------------------------------

export interface PrimitiveBridgeInput {
  irName: string;
  irArgSorts: SortRef[];
  irReturnSort: SortRef;
  sourceLayer: string;
  targetContractCid: string;
  targetLayer: string;
  notes?: string;
}

/**
 * Declare a primitive bridge. Returns a callable that builds IR ctor
 * nodes referencing the bridged name. Registers the bridge declaration
 * so the verifier can resolve through it to the deeper-layer's
 * authority.
 *
 * Idempotent for byte-identical re-registration; throws on collision
 * with a different declaration body.
 */
export function primitiveBridge(
  input: PrimitiveBridgeInput,
): (...args: Array<IrTerm | number | bigint | string | boolean>) => IrTerm {
  const decl: PrimitiveBridgeDeclaration = {
    irName: input.irName,
    irArgSorts: input.irArgSorts,
    irReturnSort: input.irReturnSort,
    sourceLayer: input.sourceLayer,
    targetContractCid: input.targetContractCid,
    targetLayer: input.targetLayer,
    ...(input.notes ? { notes: input.notes } : {}),
  };

  const existing = state.byIrName.get(input.irName);
  if (existing) {
    if (JSON.stringify(existing) !== JSON.stringify(decl)) {
      throw new Error(
        `Primitive bridge "${input.irName}" is already registered with a ` +
          `different target. The kit cannot bridge the same IR name to two ` +
          `different deeper-layer authorities.`,
      );
    }
  } else {
    state.byIrName.set(input.irName, decl);
  }

  const returnSort = resolveSort(input.irReturnSort);
  return (...args): IrTerm => ({
    kind: "ctor",
    name: input.irName,
    args: args.map((a) => liftToTerm(a as IrTerm | number | bigint | string | boolean)),
    sort: returnSort,
  });
}

function resolveSort(ref: SortRef): Sort {
  if (typeof ref === "string") {
    return { kind: "primitive", name: ref };
  }
  return ref;
}
