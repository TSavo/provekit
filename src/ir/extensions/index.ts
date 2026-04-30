/**
 * Extension authoring + registry — public surface.
 *
 * See docs/specs/2026-04-30-ir-extension-protocol.md.
 *
 * The framework's commitment: the IR is total via the protocol.
 * Anything that can't be expressed in bootstrapping core (Int, Real,
 * Bool, String, set/tuple/function compounds, the standard predicates
 * and connectives) is an extension. Extensions are authored through
 * this module, not by editing the framework.
 *
 * The kit's own "built-in" primitives are themselves extensions
 * authored through this module at load time. No two-tier system.
 * Same DX for `parseInt` and your custom `fixedPointMul`.
 */

export {
  extensionSort,
  extensionPredicate,
  extensionCtor,
  type ExtensionSortInput,
  type ExtensionPredicateInput,
  type ExtensionCtorInput,
} from "./authoring.js";

export {
  registerExtensionDeclaration,
  resolveExtension,
  lookupSort,
  lookupPredicate,
  lookupCtor,
  listExtensions,
  _resetRegistry,
  UnresolvedExtensionError,
  ExtensionRegistrationError,
  type ExtensionDeclaration,
  type SortExtensionDeclaration,
  type PredicateExtensionDeclaration,
  type CtorExtensionDeclaration,
  type SemanticDeclaration,
  type SortRef,
} from "./registry.js";
