# Built-in Contracts Catalog (TS Kit)

This directory is the seed of the TypeScript kit's built-in contract catalog. Each `.invariant.ts` file describes the behavior of a host-language built-in — `parseInt`, `Math.abs`, `Array.prototype.every`, and so on — in the same `.invariant.ts` format users write for their own code.

## What sorcery is this?

The framework cannot modify Node.js source. It cannot patch `parseInt`'s implementation. It cannot inject runtime hooks into `Math.abs`. The constraint-by-design rule applies: the artifact under verification stays bit-identical, and constraints attach EXTERNALLY as separate files.

These files ARE those external constraints. The TS kit ships with this catalog. The lifter loads it at startup. When user code calls `parseInt(userInput)`, the prover consults `parseInt.invariant.ts` to know what return values are possible — including `0`, including `NaN`, including everything in between. That's how the prover catches the divide-by-zero counterexample (see §14 of the TS-IR language spec).

**There is no separate "registry API."** The lifter has ONE input format: `.invariant.ts` files. User code lives in user files. Built-in semantics live in this catalog. Third-party library contracts live in `<project>/.provekit/contracts/`. Same format, same lifter, same prover.

## How they're authored

Each file:
- Imports the built-in symbol it describes (`parseInt`, `Math`, etc.)
- Imports `property`, `forAll`, `exists`, `implies` from `provekit/ir`
- Declares one or more `property("name", predicate)` clauses
- Each clause captures one aspect of the built-in's behavior

The properties use the same TS subset users write — operators, quantifiers via `.every`/`.some`, ternaries, optional chaining. The lifter projects them into IR identically to user invariants. A `parseInt` property and a user invariant are indistinguishable in the IR; only their source files differ.

## Why ship them at all

A kit's built-in catalog is its primary value-add. The kit author writes `parseInt`'s contract once; every codebase using the TS kit gets symbolic-range-aware verification of every `parseInt` callsite forever. The same is true for `Math.*`, `Array.prototype.*`, and so on.

This is what makes the framework cheap to extend across languages. Each kit (Rust, Go, COBOL, Python) ships its own catalog of built-ins for its host language. The IMPLEMENTATION is fungible — ProvegIt, ProverIt, whatever — but the contract format is identical because the spec mandates it.

## Coverage targets (v1)

The TS kit's catalog must cover, at minimum:

- All `Math.*` pure functions: `abs`, `max`, `min`, `floor`, `ceil`, `round`, `sign`, `pow`, `sqrt`, `log`, `exp`, `sin`, `cos`, `tan`, `PI`, `E`
- All `Number.*` static methods: `parseInt`, `parseFloat`, `isFinite`, `isInteger`, `isNaN`, `MAX_SAFE_INTEGER`, `MIN_SAFE_INTEGER`
- All `String.prototype` pure methods: `charAt`, `charCodeAt`, `slice`, `substring`, `length`, `indexOf`, `lastIndexOf`, `includes`, `startsWith`, `endsWith`, `toLowerCase`, `toUpperCase`, `trim`, `split`, `repeat`
- All `Array.prototype` pure methods: `every`, `some`, `find`, `findIndex`, `includes`, `indexOf`, `length`, `at`, `slice`, `concat`, `flat`, `flatMap`, `map`, `filter`, `reduce`, `entries`, `keys`, `values`
- Bare globals: `parseInt`, `parseFloat`, `isNaN`, `isFinite`, `String`, `Number`, `Boolean`, `JSON.parse`, `JSON.stringify`

The files in this directory demonstrate the format with three illustrative examples; the full catalog ships when the lifter implementation lands.

## Status

These files use the SURFACE form described in `2026-04-29-ts-ir-language.md`. The lifter that consumes this surface is pending implementation. Until the lifter exists, these files are spec artifacts — not type-checked by `tsc` against the current `provekit/ir` library, which exposes a different (BUILDER) API.

When the lifter implementation lands:
1. These files move to `src/builtins/`
2. They become the real built-in catalog the kit ships
3. They are loaded by the lifter at startup
4. Every user codebase using the TS kit gets their semantics for free
