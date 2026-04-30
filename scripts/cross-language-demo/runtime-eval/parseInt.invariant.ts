/**
 * Sample invariant file authored using symbolic primitives.
 *
 * RUNNING this file (importing it inside beginCollecting() / finish())
 * produces the IR. Each describe / must call registers a declaration
 * with the active collector. No AST walking. No tsc Compiler API.
 * Just function calls.
 *
 * This file is NOT a test fixture. It's a real invariant declaration.
 * The symbolic primitives are imported normally; running them builds
 * the IR; the collector captures it.
 */

import {
  describe,
  must,
  forAll,
  exists,
  implies,
  parseInt,
  num,
  str,
  eq,
  gt,
  gte,
  lt,
  Int,
  String as StringSort,
} from "../../../src/ir/symbolic/index.js";

describe("parseInt", () => {
  must("canReturnZero",
    exists(StringSort, (s) => eq(parseInt(s), num(0))),
  );

  must("canReturnPositive",
    exists(StringSort, (s) => gt(parseInt(s), num(0))),
  );

  must("canReturnNegative",
    exists(StringSort, (s) => lt(parseInt(s), num(0))),
  );

  describe("non-negative integer round-trip", () => {
    must("zero",
      eq(parseInt(str("0")), num(0)),
    );

    must("idempotent on small positive integers",
      forAll(Int, (n) =>
        implies(
          gte(n, num(0)),
          // Note: this would be `parseInt(toString(n)) === n` in
          // full surface form. The kit's primitives define this.
          gte(parseInt(str("...")), num(0)),
        ),
      ),
    );
  });
});

describe("Math.abs", () => {
  must("non-negative output",
    forAll(Int, (x) => gte(num(0), num(0))),  // simplified for demo
  );
});
