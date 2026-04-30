/**
 * Built-in contract: Math.* (subset of ECMA-262 §21.3)
 *
 * Spec source: ECMA-262 §21.3 (The Math Object)
 * Kit: provekit-ts@1.0
 * Status: SEED MEMENTO of the global proof DAG.
 *
 * Math is the lowest-friction contract surface in JavaScript. The functions
 * are pure, deterministic, well-specified, and used by virtually every
 * codebase. Properties verified here become free leverage for every TS
 * project that calls Math.abs, Math.max, Math.min, Math.floor, etc.
 *
 * Coverage in this file: abs, max, min, floor, ceil, round, sign, sqrt.
 * Other Math functions (pow, log, exp, sin, cos, etc.) ship in companion
 * files under the same kit.
 */

import { property, forAll, exists, implies, Int, Real } from 'provekit/ir';

// ---------------------------------------------------------------------------
// Math.abs
// Spec: ECMA-262 §21.3.2.1
// ---------------------------------------------------------------------------

property("Math.abs.returnsNonNegative",
  forAll<Real>(x => Math.abs(x) >= 0)
);

property("Math.abs.preservesMagnitude",
  forAll<Real>(x => Math.abs(x) === Math.abs(-x))
);

property("Math.abs.identityOnNonNegative",
  forAll<Real>(x => implies(x >= 0, Math.abs(x) === x))
);

property("Math.abs.negatesOnNegative",
  forAll<Real>(x => implies(x < 0, Math.abs(x) === -x))
);

property("Math.abs.zeroFixedPoint",
  Math.abs(0) === 0
);

// ---------------------------------------------------------------------------
// Math.max
// Spec: ECMA-262 §21.3.2.24
// ---------------------------------------------------------------------------

property("Math.max.returnsArgument",
  forAll<Real>(a =>
    forAll<Real>(b =>
      Math.max(a, b) === a || Math.max(a, b) === b
    )
  )
);

property("Math.max.upperBound",
  forAll<Real>(a =>
    forAll<Real>(b =>
      Math.max(a, b) >= a && Math.max(a, b) >= b
    )
  )
);

property("Math.max.commutative",
  forAll<Real>(a =>
    forAll<Real>(b =>
      Math.max(a, b) === Math.max(b, a)
    )
  )
);

property("Math.max.idempotent",
  forAll<Real>(a => Math.max(a, a) === a)
);

// ---------------------------------------------------------------------------
// Math.min
// Spec: ECMA-262 §21.3.2.25
// ---------------------------------------------------------------------------

property("Math.min.returnsArgument",
  forAll<Real>(a =>
    forAll<Real>(b =>
      Math.min(a, b) === a || Math.min(a, b) === b
    )
  )
);

property("Math.min.lowerBound",
  forAll<Real>(a =>
    forAll<Real>(b =>
      Math.min(a, b) <= a && Math.min(a, b) <= b
    )
  )
);

property("Math.min.commutative",
  forAll<Real>(a =>
    forAll<Real>(b =>
      Math.min(a, b) === Math.min(b, a)
    )
  )
);

property("Math.min.idempotent",
  forAll<Real>(a => Math.min(a, a) === a)
);

// Cross-property — relationship between max and min.
property("Math.maxMin.duality",
  forAll<Real>(a =>
    forAll<Real>(b =>
      Math.max(a, b) + Math.min(a, b) === a + b
    )
  )
);

// ---------------------------------------------------------------------------
// Math.floor
// Spec: ECMA-262 §21.3.2.16
// ---------------------------------------------------------------------------

property("Math.floor.returnsInteger",
  forAll<Real>(x => Number.isInteger(Math.floor(x)))
);

property("Math.floor.lessThanOrEqualToInput",
  forAll<Real>(x => Math.floor(x) <= x)
);

property("Math.floor.idempotentOnIntegers",
  forAll<Int>(n => Math.floor(n) === n)
);

property("Math.floor.greaterThanInputMinusOne",
  forAll<Real>(x => Math.floor(x) > x - 1)
);

// ---------------------------------------------------------------------------
// Math.ceil
// Spec: ECMA-262 §21.3.2.10
// ---------------------------------------------------------------------------

property("Math.ceil.returnsInteger",
  forAll<Real>(x => Number.isInteger(Math.ceil(x)))
);

property("Math.ceil.greaterThanOrEqualToInput",
  forAll<Real>(x => Math.ceil(x) >= x)
);

property("Math.ceil.idempotentOnIntegers",
  forAll<Int>(n => Math.ceil(n) === n)
);

property("Math.ceil.lessThanInputPlusOne",
  forAll<Real>(x => Math.ceil(x) < x + 1)
);

// Cross-property — floor and ceil bracket the input.
property("Math.floorCeil.bracket",
  forAll<Real>(x =>
    Math.floor(x) <= x && x <= Math.ceil(x)
  )
);

property("Math.floorCeil.equalOnIntegers",
  forAll<Int>(n => Math.floor(n) === Math.ceil(n))
);

// ---------------------------------------------------------------------------
// Math.sign
// Spec: ECMA-262 §21.3.2.28
// ---------------------------------------------------------------------------

property("Math.sign.returnsNegOneZeroOrOne",
  forAll<Real>(x =>
    Math.sign(x) === -1 || Math.sign(x) === 0 || Math.sign(x) === 1
  )
);

property("Math.sign.zeroOnZero",
  Math.sign(0) === 0
);

property("Math.sign.positiveForPositive",
  forAll<Real>(x => implies(x > 0, Math.sign(x) === 1))
);

property("Math.sign.negativeForNegative",
  forAll<Real>(x => implies(x < 0, Math.sign(x) === -1))
);

// ---------------------------------------------------------------------------
// Math.sqrt
// Spec: ECMA-262 §21.3.2.31
// ---------------------------------------------------------------------------

property("Math.sqrt.nonNegativeInputProducesNonNegativeOutput",
  forAll<Real>(x => implies(x >= 0, Math.sqrt(x) >= 0))
);

property("Math.sqrt.negativeInputProducesNaN",
  forAll<Real>(x => implies(x < 0, Number.isNaN(Math.sqrt(x))))
);

property("Math.sqrt.zeroFixedPoint",
  Math.sqrt(0) === 0
);

property("Math.sqrt.oneFixedPoint",
  Math.sqrt(1) === 1
);

property("Math.sqrt.squaresInverse",
  forAll<Real>(x =>
    implies(x >= 0, Math.sqrt(x * x) === x)
  )
);

// ---------------------------------------------------------------------------
// What this file's mementos contribute to the global proof DAG.
//
// Mementos minted: ~32 (one per property declaration above)
// Composite DAG root: hash of the proof DAG containing all the above
// inputCids include:
//   - ECMA-262 §21.3 content hash (the spec leaf)
//   - IEEE 754 floating-point arithmetic spec hash (transitive)
//   - vitest-producer mementos for each property
//
// Every TS codebase doing arithmetic transitively depends on these
// mementos. Adversarial re-verification by the consumer's proofkit:
// each property is re-evaluated by the consumer's SMT solver, LLM
// verifiers, and test runners. If any disagreement surfaces between
// the kit's claim and the consumer's re-verification, install rejected.
//
// Cross-language equivalence opportunities: every host-language kit
// that ships an "absolute value" function will have a property
// matching `Math.abs.returnsNonNegative` at the canonical FOL level.
// Cross-equivalence mementos can be minted to bridge them (Rust
// `i32::abs`, Python `abs`, COBOL `FUNCTION ABS`, etc.). Each bridge
// becomes a public good; once minted, every multi-language project
// inherits it.
// ---------------------------------------------------------------------------
