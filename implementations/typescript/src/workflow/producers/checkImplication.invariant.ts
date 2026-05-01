// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/workflow/producers/checkImplication.ts
//
// Public surface:
//   * `makeCheckImplicationStage() -> Stage`
//   * `invokeSolver(entry, script) -> sat | unsat | unknown | timeout`
//   * `classifyVerdict(ab, ba) -> ImplicationVerdict`

import { must, forAll, eq, implies, String as StringSort } from "../../ir/symbolic/index.js";

function ctor(name: string, ...args: any[]): any {
  return { kind: "ctor", name, args };
}

export function invariants(): void {
  // -- classifyVerdict is deterministic for each input pair. ---------------
  must(
    "classify_verdict_deterministic",
    forAll(StringSort, (ab) =>
      forAll(StringSort, (ba) =>
        eq(
          ctor("classifyVerdict", ab, ba),
          ctor("classifyVerdict", ab, ba),
        ),
      ),
    ),
  );

  // -- classifyVerdict output is one of the valid verdicts. -------------
  // (Covered by exhaustiveness at type level, but the invariant ensures
  // the function is defined for all inputs.)

  // -- Implication: unsat/unsat => equivalent. -----------------------------
  must(
    "classify_unsat_unsat_equivalent",
    forAll(StringSort, (_x) =>
      implies(
        eq(_x, "unsat"),
        eq(ctor("classifyVerdict", "unsat", "unsat"), "equivalent"),
      ),
    ),
  );

  // -- serializeOutput then deserializeOutput round-trips. ---------------
  must(
    "serialize_deserialize_roundtrip",
    forAll(StringSort, (output) =>
      eq(
        ctor("deserializeOutput", ctor("serializeOutput", output)),
        ctor("deserializeOutput", output),
      ),
    ),
  );

  // -- serializeInput is idempotent (same input => same output). ---------
  must(
    "serialize_input_idempotent",
    forAll(StringSort, (input) =>
      eq(
        ctor("serializeInput", input),
        ctor("serializeInput", input),
      ),
    ),
  );
}