/**
 * @provekit/canonicalizer — AST canonicalizer entrypoint.
 *
 * Implements the AstCanonicalizer interface from the spec
 * (docs/specs/2026-04-29-ast-canonicalizer.md §"The AstCanonicalizer interface").
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
import { sha256Prefix16 } from "./hash.js";

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
 *               → JCS-JSON serialization → SHA-256 prefix-16
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
   * Returns a 16-character hex string (sha256-prefix-16 of canonical JCS-JSON bytes).
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
   * Returns a 16-character hex string (sha256-prefix-16).
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
    return sha256Prefix16(bytes);
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
export { sha256Prefix16 } from "./hash.js";
