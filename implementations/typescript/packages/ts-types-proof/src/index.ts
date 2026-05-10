/**
 * @provekit/ts-types-proof: Trojan-horse shim that delivers the
 * ProvekIt protocol substrate to TypeScript projects via a single
 * `npm install`.
 *
 * The user installs this package because they want typed wrappers
 * for parseInt / Math.* / String.* / Array.*. They get those: same
 * shape they'd find in any TypeScript primitive-helper package. As
 * a SIDE EFFECT, this package's import registers V8 bridge
 * declarations in the protocol's process-local registry, so any
 * invariant the user authors (via `must(...)`, `forAll(...)`, etc.)
 * automatically composes into the proofHash chain.
 *
 * Adoption asymmetry: refusing the protocol means refusing typed
 * helpers. Nobody does that. The types are the gift; the protocol
 * is what's inside the horse. Once installed, the user's code
 * participates in the proofHash composition story without their
 * having to explicitly opt in.
 *
 * Deprecation roadmap: when upstream ts-types (the ecosystem's
 * canonical types package) adopts the protocol natively, this shim
 * deprecates with a one-line dependency swap. PropertyHash CIDs are
 * stable across the migration because content addressing: the
 * shim's parseInt produces the same CID as a protocol-native
 * ts-types' parseInt, since both bridge to V8's parseInt
 * declaration via the same canonical IR.
 *
 * Re-exports `@provekit/ir-symbolic` verbatim. Side-effect: bridge
 * registration happens at import time via that module's lazy-init
 * machinery (commit 2080fa5).
 */

// Re-export the symbolic primitives kit. Side effect: importing the
// kit registers all 13 V8 bridge declarations at module load.
export {
  // Constants
  num,
  real,
  str,
  bool,
  // Bridged ECMA-262 primitives
  parseInt,
  parseFloat,
  isNaN,
  isFinite,
  isInteger,
  floor,
  ceil,
  sqrt,
  sign,
  stringLength,
  stringIncludes,
  arrayLength,
  arrayIncludes,
  // Polymorphic Math.* (still raw ctors; tracked for per-sort split)
  abs,
  max,
  min,
  // Connectives + quantifiers
  // (pulled in transitively when users author invariants)
} from "../../../src/ir/symbolic/primitives.js";

// Re-export the IR types so consumers can write typed signatures.
export type { IrFormula, IrTerm, Sort } from "../../../src/ir/formulas.js";

// Re-export extension authoring + bridge factories for users who
// want to declare their own. The protocol's design is open: new
// sorts/predicates/ctors are published declarations, not framework
// edits.
export {
  extensionSort,
  extensionPredicate,
  extensionCtor,
  primitiveBridge,
  listExtensions,
  listBridges,
} from "../../../src/ir/extensions/index.js";

/**
 * The shim's identifying CID. When this package's bridge declarations
 * are signed (post v0.1.0), this CID names the signed catalog of
 * V8 bridges. Today's value is a placeholder; signing requires the
 * upstream catalog publishing infrastructure to be live.
 */
export const TS_TYPES_PROOF_VERSION = "0.1.0";
export const TS_TYPES_PROOF_CATALOG_CID = "bafy_TS_TYPES_PROOF_PLACEHOLDER_CID";
