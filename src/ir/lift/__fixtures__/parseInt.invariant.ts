/**
 * Trimmed parseInt fixture for lifter tests. Mirrors the catalog spine
 * (docs/specs/builtins-catalog/parseInt.invariant.ts) at a smaller
 * surface area so the lifter test suite has stable change cadence.
 */

import { property, forAll, exists, implies } from "provekit/ir";
import type { Int, StringSort } from "provekit/sorts";

property(
  "parseIntCanReturnZero",
  exists<StringSort>((s) => parseInt(s) === 0),
);

property(
  "parseIntCanReturnNaN",
  exists<StringSort>((s) => Number.isNaN(parseInt(s))),
);

property(
  "parseIntCanReturnPositiveInteger",
  exists<StringSort>((s) => parseInt(s) > 0),
);

property("parseIntZeroStringIsZero", parseInt("0") === 0);

property("parseIntEmptyStringIsNaN", Number.isNaN(parseInt("")));

property(
  "parseIntReturnsIntOrNaN",
  forAll<StringSort>(
    (s) => Number.isInteger(parseInt(s)) || Number.isNaN(parseInt(s)),
  ),
);

property(
  "parseIntIsDeterministic",
  forAll<StringSort>((s) => parseInt(s) === parseInt(s)),
);

property(
  "parseIntPreservesNonNegativeIntegers",
  forAll<Int>((n) => implies(n >= 0, parseInt(String(n)) === n)),
);
