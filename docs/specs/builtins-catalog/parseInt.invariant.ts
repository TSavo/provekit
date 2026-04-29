/**
 * Built-in contract: parseInt
 *
 * Spec source: ECMA-262 §19.2.5 (Number conversion: parseInt)
 * Kit: provekit-ts@1.0
 * Status: SEED MEMENTO of the global proof DAG.
 *
 * This file describes the behavior of the JavaScript / Node.js / V8 built-in
 * `parseInt`. Every TypeScript codebase using parseInt transitively depends
 * on the mementos minted from this file. Their content hashes will appear
 * in the inputCids of millions of downstream verification mementos.
 *
 * Get this right. Get it WRONG and the entire ecosystem inherits the bug.
 *
 * The kit author signs each property with the kit's producer key. Consumers
 * adversarially re-verify under their own proofkit before trusting any
 * verdict. The signature attests to identity; the consumer's re-verification
 * attests to validity.
 *
 * Spec form (when the lifter ships, this file moves to src/builtins/ and
 * is consumed verbatim by the TS-kit's lifter). Until then, this file is
 * SPECIFICATION as code — it documents the format, the properties, and the
 * audit trail. It does not type-check against the current builder API.
 */

import { property, forAll, exists, implies, Int, StringSort } from 'provekit/ir';

// ---------------------------------------------------------------------------
// Existence properties — what parseInt's range CAN include.
// These are load-bearing for shadow AST walking: when the prover encounters
// `parseInt(userInput)` in user code, it consults the existence properties
// to determine the symbolic value range. The divide-by-zero counterexample
// in the spec (§14 of ts-ir-language) is driven by `parseIntCanReturnZero`.
// ---------------------------------------------------------------------------

property("parseIntCanReturnZero",
  exists<StringSort>(s => parseInt(s) === 0)
);

property("parseIntCanReturnNaN",
  exists<StringSort>(s => Number.isNaN(parseInt(s)))
);

property("parseIntCanReturnPositiveInteger",
  exists<StringSort>(s => parseInt(s) > 0)
);

property("parseIntCanReturnNegativeInteger",
  exists<StringSort>(s => parseInt(s) < 0)
);

// ---------------------------------------------------------------------------
// Specific input/output relationships — pinned point cases.
// These act as fixtures: the prover can use them as concrete values when
// symbolic reasoning needs grounding. Compliance auditors verify each
// against ECMA-262 directly.
// ---------------------------------------------------------------------------

property("parseIntZeroStringIsZero",
  parseInt("0") === 0
);

property("parseIntZeroPaddedStringIsZero",
  parseInt("00") === 0
);

property("parseIntEmptyStringIsNaN",
  Number.isNaN(parseInt(""))
);

property("parseIntWhitespaceOnlyIsNaN",
  Number.isNaN(parseInt("   "))
);

property("parseIntNonNumericIsNaN",
  Number.isNaN(parseInt("hello"))
);

// ---------------------------------------------------------------------------
// Universal properties — what parseInt guarantees for ALL inputs in domain.
// These compose into invariants that downstream callsites depend on.
// ---------------------------------------------------------------------------

property("parseIntReturnsIntOrNaN",
  forAll<StringSort>(s =>
    Number.isInteger(parseInt(s)) || Number.isNaN(parseInt(s))
  )
);

property("parseIntIsDeterministic",
  forAll<StringSort>(s => parseInt(s) === parseInt(s))
);

property("parseIntPreservesNonNegativeIntegers",
  forAll<Int>(n =>
    implies(n >= 0, parseInt(String(n)) === n)
  )
);

property("parseIntPreservesNegativeIntegers",
  forAll<Int>(n =>
    implies(n < 0, parseInt(String(n)) === n)
  )
);

// ---------------------------------------------------------------------------
// Behavioral edge cases — pinned because they are common bug sources.
// Each property here was chosen because it represents a known surprise in
// parseInt's behavior. Documenting them as invariants converts surprise
// into mechanical verification.
// ---------------------------------------------------------------------------

property("parseIntTruncatesFractionalPart",
  parseInt("3.7") === 3
);

property("parseIntStopsAtFirstNonDigit",
  parseInt("42abc") === 42
);

property("parseIntIgnoresLeadingWhitespace",
  parseInt("  42") === 42
);

property("parseIntHandlesLeadingPlus",
  parseInt("+42") === 42
);

property("parseIntHandlesLeadingMinus",
  parseInt("-42") === -42
);

// ---------------------------------------------------------------------------
// What this file's mementos contribute to the global proof DAG.
//
// inputCids of the mementos minted from this file include:
//   - ECMA-262 §19.2.5 source-text content hash (the spec leaf)
//   - vitest-producer mementos for each property (empirical demonstrations)
//   - tsc-producer memento confirming this file type-checks
//
// Mementos minted: ~17 (one per property declaration above)
// Composite DAG root: hash of the proof DAG containing all the above
// Maintainer signature: ed25519 over the canonicalized DAG, signed by
//                       provekit-ts-kit@1.0 producer key
//
// Downstream consumers (every TS codebase that calls parseInt) walk to
// THIS file's mementos when verifying their callsites. Adversarial
// re-verification: the consumer's proofkit re-runs each property under
// its own producer pool. If any property fails to re-verify, the consumer
// rejects the kit catalog and refuses to compose.
// ---------------------------------------------------------------------------
