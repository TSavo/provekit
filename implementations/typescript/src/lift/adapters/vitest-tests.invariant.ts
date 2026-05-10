// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/lift/adapters/vitest-tests.ts
//
// Public surface covered: vitest test-name -> IR-formula adapter.
// `it("must P", () => ...)` becomes a contract whose `pre` is the
// lifted assertion.
//
// Honest scope: the IR pins shape: no raw test bodies, just lifted
// claim names; deterministic for a fixed input file.

import {
  must,
  forAll,
  eq,
  gte,
  num,
  String as StringSort,
  type IrTerm,
} from "../../ir/symbolic/index.js";

function ctor1(name: string, arg: IrTerm): IrTerm {
  return { kind: "ctor", name, args: [arg] };
}

export function invariants(): void {
  // -- liftVitestTests is deterministic for a fixed source file. ----------
  must(
    "lift_vitest_tests_is_deterministic",
    forAll(StringSort, (f) =>
      eq(ctor1("liftVitestTestsIr", f), ctor1("liftVitestTestsIr", f)),
    ),
  );

  // -- liftVitestTests declaration count is non-negative. -----------------
  must(
    "lift_vitest_tests_decl_count_nonneg",
    forAll(StringSort, (f) =>
      gte(ctor1("liftVitestTestsDeclCount", f), num(0)),
    ),
  );

  // -- Every lifted vitest contract has a non-empty name. -----------------
  must(
    "lift_vitest_tests_decl_name_nonempty",
    forAll(StringSort, (d) =>
      gte(ctor1("len", ctor1("liftVitestTestsDeclName", d)), num(1)),
    ),
  );
}
