/**
 * Trimmed Math fixture for lifter tests. Mirrors the catalog spine
 * (protocol/specs/builtins-catalog/Math.invariant.ts).
 */

import { property, forAll, implies } from "provekit/ir";
import type { Int, Real } from "provekit/sorts";

property("Math.abs.returnsNonNegative", forAll<Real>((x) => Math.abs(x) >= 0));

property(
  "Math.abs.preservesMagnitude",
  forAll<Real>((x) => Math.abs(x) === Math.abs(-x)),
);

property(
  "Math.abs.identityOnNonNegative",
  forAll<Real>((x) => implies(x >= 0, Math.abs(x) === x)),
);

property("Math.abs.zeroFixedPoint", Math.abs(0) === 0);

property(
  "Math.max.commutative",
  forAll<Real>((a) => forAll<Real>((b) => Math.max(a, b) === Math.max(b, a))),
);

property(
  "Math.floor.idempotentOnIntegers",
  forAll<Int>((n) => Math.floor(n) === n),
);
