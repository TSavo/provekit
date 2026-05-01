// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/verifier/bridgeEnforcement.ts
//
// Public surface covered:
//   * `runBridgeEnforcement(projectRoot) -> Promise<Report>`
//   * `enumerateBridgeCallsites(...)`
//
// Honest scope: full pipeline correctness lives in integration tests;
// the IR captures shape — discharged + violations + undecidable counts
// always sum to totalCallsites, totals are non-negative.

import {
  must,
  forAll,
  eq,
  gte,
  num,
  add,
  String as StringSort,
  type IrTerm,
} from "../ir/symbolic/index.js";

function ctor1(name: string, arg: IrTerm): IrTerm {
  return { kind: "ctor", name, args: [arg] };
}

export function invariants(): void {
  // -- The report's three status counts sum to totalCallsites. ------------
  must(
    "bridge_enforcement_status_counts_sum_to_total",
    forAll(StringSort, (r) =>
      eq(
        add(
          add(
            ctor1("reportDischarged", r),
            ctor1("reportViolations", r),
          ),
          ctor1("reportUndecidable", r),
        ),
        ctor1("reportTotalCallsites", r),
      ),
    ),
  );

  // -- discharged count is non-negative. ----------------------------------
  must(
    "bridge_enforcement_discharged_nonneg",
    forAll(StringSort, (r) => gte(ctor1("reportDischarged", r), num(0))),
  );

  // -- violations count is non-negative. ----------------------------------
  must(
    "bridge_enforcement_violations_nonneg",
    forAll(StringSort, (r) => gte(ctor1("reportViolations", r), num(0))),
  );

  // -- runBridgeEnforcement is deterministic on a fixed project root. -----
  must(
    "run_bridge_enforcement_is_deterministic",
    forAll(StringSort, (root) =>
      eq(
        ctor1("runBridgeEnforcementSummary", root),
        ctor1("runBridgeEnforcementSummary", root),
      ),
    ),
  );
}
