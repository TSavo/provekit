/**
 * Extension registry — process-local store of extension declarations.
 *
 * Per the IR extension protocol (protocol/specs/2026-04-30-ir-extension-protocol.md):
 * a kit that authors invariants using extensions registers each extension
 * declaration here, then emits IR formulas referencing the extension by
 * name. Verifiers consult this registry (via the resolver) to look up
 * declarations when validating IR.
 *
 * The registry is process-local. Production deployments use the
 * memento store (.provekit/) as the durable home for declarations;
 * this in-memory registry is the kit's runtime cache.
 *
 * Reset semantics mirror the symbolic-primitives collector:
 * `_resetRegistry()` clears state so test runs start fresh.
 */

import type { Sort } from "../formulas.js";

// ---------------------------------------------------------------------------
// Public types — match the extension-declaration memento shape
// ---------------------------------------------------------------------------

export type SemanticDeclaration =
  | { kind: "smt-lib-theory"; theory: string; version?: string }
  | { kind: "axiom-set"; axioms: unknown[] }
  | { kind: "proof-assistant"; system: string; identifier: string; proofCid?: string }
  | { kind: "natural-language"; text: string };

export interface SortExtensionDeclaration {
  introduces: "sort";
  name: string;
  params?: Array<{ name: string; paramSort: "Int" | "Bool" | "String" }>;
  semantics: SemanticDeclaration[];
  compilers: string[];
  signer?: string;     // CID of signer's pubkey memento; optional during authoring
  signature?: string;  // Ed25519 signature; optional during authoring
  declaredAt?: string;
  dependsOn?: string[];
}

export interface PredicateExtensionDeclaration {
  introduces: "predicate";
  name: string;
  argSorts: SortRef[];
  semantics: SemanticDeclaration[];
  compilers: string[];
  signer?: string;
  signature?: string;
  declaredAt?: string;
  dependsOn?: string[];
}

export interface CtorExtensionDeclaration {
  introduces: "ctor";
  name: string;
  argSorts: SortRef[];
  returnSort: SortRef;
  semantics: SemanticDeclaration[];
  compilers: string[];
  signer?: string;
  signature?: string;
  declaredAt?: string;
  dependsOn?: string[];
}

export type ExtensionDeclaration =
  | SortExtensionDeclaration
  | PredicateExtensionDeclaration
  | CtorExtensionDeclaration;

/**
 * Sort reference inside an extension's signature. References to
 * bootstrapping core sorts use their literal name string ("Int", "Real",
 * "Bool", "String", or compound shapes); references to other extension
 * sorts use the Sort value (which carries the extension's name).
 */
export type SortRef = string | Sort;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

export class UnresolvedExtensionError extends Error {
  constructor(
    public readonly name: string,
    public readonly kind: "sort" | "predicate" | "ctor",
    public readonly reason:
      | "no-declaration"
      | "name-collision"
      | "compiler-incompatible"
      | "dependency-unresolvable"
      | "cyclic-dependency",
  ) {
    super(
      `Extension ${kind} "${name}" did not resolve: ${reason}. ` +
        `Per the IR extension protocol, the verifier MUST fail closed when an ` +
        `extension name has no declaration in scope, has multiple incompatible ` +
        `declarations, or doesn't match the active compiler.`,
    );
    this.name = "UnresolvedExtensionError";
  }
}

export class ExtensionRegistrationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ExtensionRegistrationError";
  }
}

// ---------------------------------------------------------------------------
// Registry state
// ---------------------------------------------------------------------------

interface RegistryState {
  byName: Map<string, ExtensionDeclaration>;
}

let state: RegistryState = { byName: new Map() };

/**
 * Register an extension declaration. Throws if the name is already
 * registered with a different declaration body. Idempotent for byte-
 * identical re-registration.
 */
export function registerExtensionDeclaration(decl: ExtensionDeclaration): void {
  const existing = state.byName.get(decl.name);
  if (existing) {
    if (JSON.stringify(existing) !== JSON.stringify(decl)) {
      throw new ExtensionRegistrationError(
        `Extension "${decl.name}" is already registered with a different ` +
          `declaration. Re-registering with a different body is a registry ` +
          `collision; consider giving the new extension a distinct name.`,
      );
    }
    return; // idempotent
  }
  state.byName.set(decl.name, decl);
}

/**
 * Look up a sort extension by name. Returns null if no declaration is
 * registered or if the registered declaration is not a sort.
 */
export function lookupSort(name: string): SortExtensionDeclaration | null {
  const decl = state.byName.get(name);
  if (!decl || decl.introduces !== "sort") return null;
  return decl;
}

export function lookupPredicate(name: string): PredicateExtensionDeclaration | null {
  const decl = state.byName.get(name);
  if (!decl || decl.introduces !== "predicate") return null;
  return decl;
}

export function lookupCtor(name: string): CtorExtensionDeclaration | null {
  const decl = state.byName.get(name);
  if (!decl || decl.introduces !== "ctor") return null;
  return decl;
}

/**
 * List every registered extension declaration. Order is insertion order;
 * callers that need a deterministic order should sort by name.
 */
export function listExtensions(): ExtensionDeclaration[] {
  return [...state.byName.values()];
}

/**
 * Resolve an extension by name. Implements the resolver semantics from
 * the IR extension protocol (§5.1):
 *
 *   1. Look up the declaration.
 *   2. Check the active compiler is in the declaration's compilers list.
 *   3. (Future: verify signature, check key revocation, walk deps.)
 *
 * The signature/revocation checks are stubs today (return true) until
 * the signatures spec's reference implementation lands; the contract is
 * documented in the protocol spec and conformance is checked at the
 * verifier layer. The compiler-compat check is mechanical and runs now.
 *
 * Throws `UnresolvedExtensionError` per the fail-closed gate.
 */
export function resolveExtension(
  name: string,
  kind: "sort" | "predicate" | "ctor",
  context: { activeCompiler: string },
): ExtensionDeclaration {
  const decl = state.byName.get(name);
  if (!decl) {
    throw new UnresolvedExtensionError(name, kind, "no-declaration");
  }
  if (decl.introduces !== kind) {
    throw new UnresolvedExtensionError(name, kind, "no-declaration");
  }
  if (!decl.compilers.includes(context.activeCompiler)) {
    throw new UnresolvedExtensionError(name, kind, "compiler-incompatible");
  }
  return decl;
}

/**
 * Reset the registry. Tests call this between cases. Production callers
 * typically don't; the registry persists for the process lifetime.
 */
export function _resetRegistry(): void {
  state = { byName: new Map() };
}
