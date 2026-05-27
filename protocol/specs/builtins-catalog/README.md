# Built-in Contracts Catalog (TS Kit)

This directory used to carry hand-authored TypeScript contract examples
for host-language built-ins. That format is retired. Built-in contracts
must now enter the catalog the same way all other contracts do: lifted
from native source, native tests, native annotations, or bridged to a
published lower-layer proof.

## What gets cataloged

The framework cannot modify Node.js source. It cannot patch `parseInt`'s implementation. It cannot inject runtime hooks into `Math.abs`. The constraint-by-design rule applies: the artifact under verification stays bit-identical, and constraints attach EXTERNALLY as separate files.

For built-ins, that external constraint is not a bespoke repo-local DSL
file. It is one of:

- a proof bundle lifted from the native implementation source, such as
  V8's `parseInt` implementation;
- a proof bundle lifted from the native conformance tests for the
  implementation or standard library;
- a bridge memento from the TypeScript surface symbol to a lower-layer
  contract, such as `global.parseInt` -> V8 -> ECMA-262 -> IEEE 754.

When user code calls `parseInt(userInput)`, the prover consults the
catalog entry for the TS surface symbol and walks the bridge chain to
the engine/spec contract. That is how the prover catches the
divide-by-zero counterexample: `parseInt` can return `0`, and the
consumer callsite must still satisfy the callee's non-zero precondition.

## How entries are authored

Each catalog entry records what native source or published proof it came
from. A TypeScript kit entry for `Math.abs`, for example, should point at
the JS engine or standard-library proof that establishes the behavior,
not restate that behavior in a kit-authored contract file.

Bridge entries are small because the behavioral work belongs to the
canonical lower layer. The TS kit's job is to attest that its surface
symbol realizes the lower-layer contract and to compose by content hash.

## Why ship them at all

A kit's built-in catalog is its primary value-add. The kit author writes `parseInt`'s contract once; every codebase using the TS kit gets symbolic-range-aware verification of every `parseInt` callsite forever. The same is true for `Math.*`, `Array.prototype.*`, and so on.

This is what makes the framework cheap to extend across languages. Each
kit (Rust, Go, COBOL, Python) ships its own catalog of built-ins for its
host language. The implementation is fungible because the boundary is the
lifted ProofIR and the content-addressed contract/bridge DAG, not a
shared hand-authored source format.

## Coverage targets (v1)

The TS kit's catalog must cover, at minimum:

- All `Math.*` pure functions: `abs`, `max`, `min`, `floor`, `ceil`, `round`, `sign`, `pow`, `sqrt`, `log`, `exp`, `sin`, `cos`, `tan`, `PI`, `E`
- All `Number.*` static methods: `parseInt`, `parseFloat`, `isFinite`, `isInteger`, `isNaN`, `MAX_SAFE_INTEGER`, `MIN_SAFE_INTEGER`
- All `String.prototype` pure methods: `charAt`, `charCodeAt`, `slice`, `substring`, `length`, `indexOf`, `lastIndexOf`, `includes`, `startsWith`, `endsWith`, `toLowerCase`, `toUpperCase`, `trim`, `split`, `repeat`
- All `Array.prototype` pure methods: `every`, `some`, `find`, `findIndex`, `includes`, `indexOf`, `length`, `at`, `slice`, `concat`, `flat`, `flatMap`, `map`, `filter`, `reduce`, `entries`, `keys`, `values`
- Bare globals: `parseInt`, `parseFloat`, `isNaN`, `isFinite`, `String`, `Number`, `Boolean`, `JSON.parse`, `JSON.stringify`

The old examples in this directory demonstrated a retired hand-authored
format. They were removed rather than translated because there is no
native Node/V8/ECMA source file in this directory to lift. The durable
catalog entries belong in the kit package that carries the native bridge
or proof-bundle generation.

## Status

This directory is now prose-only. It documents the TS built-in catalog
policy, but it does not contain contract source. The obsolete files were
not type-checked by `tsc` and were not loaded by a kept runtime path.

When the lifter implementation lands:
1. Native bridge/proof generation lives under the TypeScript kit.
2. Built-in catalog entries point at native source proofs or bridge mementos.
3. The lifter loads those entries at startup.
4. Every user codebase using the TS kit gets their semantics for free.
