/**
 * @provekit/canonicalizer — AST canonicalizer entrypoint.
 *
 * Implements the AstCanonicalizer interface from the spec
 * (protocol/specs/2026-04-29-ast-canonicalizer.md §"The AstCanonicalizer interface").
 *
 * Serialization format: canonical JSON (RFC 8785 / JCS) — the v1 fallback
 * permitted by the spec. CBOR (RFC 8949 §4.2) is the preferred format;
 * see serialize.ts for the rationale.
 *
 * Spec version: 1.0.0 (major 1 = this grammar; minor/patch additive).
 */

import type { IrFormula } from "./irFormula.js";
import type { CanonicalFolAst, CanonicalSort } from "./ast.js";
import {
  formulaToCanonicalAst,
  propertyHashFromFormula,
} from "./canonicalize.js";
import { serializeCanonicalAst, SERIALIZATION_FORMAT } from "./serialize.js";
import { computeCid } from "./hash.js";

// Re-export types for consumers.
export type { IrFormula, Sort } from "./irFormula.js";
export type {
  CanonicalFolAst,
  CanonicalSort,
  CanonicalTerm,
  CanonicalPredicate,
  CanonicalQuantifier,
  CanonicalConnective,
  CanonicalAtomic,
  CanonicalVar,
  CanonicalConst,
  CanonicalCtor,
} from "./ast.js";
export { SERIALIZATION_FORMAT } from "./serialize.js";

// -----------------------------------------------------------------------
// Stub types for SAST-integration (deferred)
// -----------------------------------------------------------------------

/**
 * A code scope (function, module, class, transition, region, etc.).
 * Full definition deferred to SAST integration.
 */
export type BindingScope = {
  kind: "function" | "module" | "class" | "method" | "transition" | "region" | "whenever" | string;
  identifier: string;
  filePath: string;
};

/**
 * Typed variable bindings within a scope.
 * Maps binding name to sort.
 */
export type Bindings = Record<string, CanonicalSort>;

/**
 * A reference to a host-language AST node. SAST integration is deferred.
 * Stub: any opaque value.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type HostAstNode = any;

// -----------------------------------------------------------------------
// AstCanonicalizerImpl
// -----------------------------------------------------------------------

/**
 * The canonical implementation of the AstCanonicalizer interface.
 *
 * Implements the full canonicalization pipeline:
 *   IrFormula → de Bruijn → predicate/sort canonicalization
 *               → implies removal → NNF → AC normalization
 *               → JCS-JSON serialization → BLAKE3-512 self-identifying hash
 *
 * STUB NOTICE: `scopeOf` and `bindingHashFromAst` are partially
 * stubbed. The SAST integration (host-AST → BindingScope extraction)
 * is a separate implementation piece. The binding hash computation
 * uses the scope + bindings data passed to it; it does not walk a
 * real SAST graph.
 */
export class AstCanonicalizerImpl {
  /**
   * Which version of the spec this canonicalizer implements.
   * Major 1: this grammar. Cross-version compatibility guaranteed within
   * the same major (minor/patch additive).
   */
  specVersion(): { major: number; minor: number; patch: number } {
    return { major: 1, minor: 0, patch: 0 };
  }

  /**
   * Compute the propertyHash for an IR formula.
   * Returns a self-identifying string of the form
   * `"blake3-512:" + hex(BLAKE3_512(canonicalJcsBytes))` (139 chars).
   */
  propertyHashFromFormula(formula: IrFormula): string {
    return propertyHashFromFormula(formula);
  }

  /**
   * Produce the canonical FOL AST for an IR formula.
   * The AST is in NNF, AC-normalized, with de Bruijn indices.
   */
  formulaToCanonicalAst(formula: IrFormula): CanonicalFolAst {
    return formulaToCanonicalAst(formula);
  }

  /**
   * Identify the canonical scope of a host AST node.
   *
   * STUB: SAST integration is deferred. This returns a placeholder
   * BindingScope. Callers relying on real scope data must wait for the
   * SAST integration pass.
   */
  scopeOf(_hostAst: HostAstNode): BindingScope {
    return {
      kind: "function",
      identifier: "__stub__",
      filePath: "__stub__",
    };
  }

  /**
   * Compute the bindingHash for a code scope.
   *
   * SIMPLIFIED: The full spec hashes scope + bindings + nodeRef, where
   * nodeRef is a SAST graph reference (deferred). This implementation
   * hashes the scope kind/identifier/filePath and the bindings map as
   * canonical JSON. The `hostAst` parameter is accepted but not used.
   *
   * Returns a self-identifying hash string
   * (`"blake3-512:" + 128 hex chars`).
   */
  bindingHashFromAst(input: {
    scope: BindingScope;
    bindings: Bindings;
    hostAst: HostAstNode;
  }): string {
    const canonicalScope = {
      kind: input.scope.kind,
      identifier: input.scope.identifier,
      filePath: input.scope.filePath,
      bindings: sortObjectKeys(input.bindings),
    };
    const json = canonicalJsonStringify(canonicalScope);
    const bytes = Buffer.from(json, "utf8");
    return computeCid(bytes);
  }
}

// -----------------------------------------------------------------------
// Default export
// -----------------------------------------------------------------------

/** The default canonicalizer instance. */
export const canonicalizer = new AstCanonicalizerImpl();

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

function sortObjectKeys<T>(obj: Record<string, T>): Record<string, T> {
  const sorted: Record<string, T> = {};
  for (const k of Object.keys(obj).sort()) {
    sorted[k] = obj[k];
  }
  return sorted;
}

/**
 * Minimal canonical JSON serializer for the bindingHash input.
 * For formula hashing, see serialize.ts.
 */
function canonicalJsonStringify(value: unknown): string {
  if (value === null || value === undefined) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") return JSON.stringify(value);
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value)) return "[" + value.map(canonicalJsonStringify).join(",") + "]";
  if (typeof value === "object") {
    const keys = Object.keys(value as Record<string, unknown>).sort();
    const pairs = keys.map(
      (k) => JSON.stringify(k) + ":" + canonicalJsonStringify((value as Record<string, unknown>)[k]),
    );
    return "{" + pairs.join(",") + "}";
  }
  return "null";
}

// Export pipeline functions directly for convenience.
export { formulaToCanonicalAst, propertyHashFromFormula } from "./canonicalize.js";
export { serializeCanonicalAst } from "./serialize.js";
export {
  blake3_512_hex,
  computeCid,
  HASH_ALGORITHM_TAG,
  HASH_PREFIX,
  SELF_IDENTIFYING_HASH_RE,
} from "./hash.js";

// -----------------------------------------------------------------------
// Protocol v1.1 hash helpers (raw IR-JSON canonical encoding).
//
// Per the contract memento spec:
//   preHash  = computeCid(canonical(pre))
//   postHash = computeCid(canonical(post))
//   invHash  = computeCid(canonical(inv))
//   propertyHash = computeCid(canonical({pre?, post?, inv?, outBinding}))
// where `canonical` is JCS over the IR-JSON shape (NOT the de Bruijn /
// AC-normalized canonical FOL AST). Same algorithm every kit uses.
// All hashes are full BLAKE3-512 self-identifying with the
// "blake3-512:" prefix.
// -----------------------------------------------------------------------

import { canonicalEncode as canonicalEncodeJcs } from "../claimEnvelope/canonicalize.js";
import { computeCid as computeCidImpl } from "./hash.js";

/**
 * Hash an IR-JSON value (formula, term, or any plain object) under JCS.
 * Returns the self-identifying string `"blake3-512:" + 128 hex chars`.
 */
export function hashIrJson(value: unknown): string {
  return computeCidImpl(canonicalEncodeJcs(value));
}

export interface ContractBodySemantics {
  pre?: unknown;
  post?: unknown;
  inv?: unknown;
  outBinding: string;
}

/**
 * Compute the wrapper-level `propertyHash` for a contract memento body.
 * Hashes the canonical encoding of {pre?, post?, inv?, outBinding} only,
 * with absent slots omitted (NOT serialized as null).
 */
export function contractPropertyHash(spec: ContractBodySemantics): string {
  const obj: Record<string, unknown> = { outBinding: spec.outBinding };
  if (spec.pre !== undefined) obj.pre = spec.pre;
  if (spec.post !== undefined) obj.post = spec.post;
  if (spec.inv !== undefined) obj.inv = spec.inv;
  return hashIrJson(obj);
}
