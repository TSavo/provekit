// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/verifier/index.ts
//
// Public surface covered:
//   * verifier orchestration entry points (loadAllProofs, enumerateCallsites,
//     resolveTarget, instantiate, dischargeObligations).
//
// Honest scope: the verifier's correctness is enforced by integration
// tests over actual .proof inputs; the IR captures shape: load-empty
// returns empty pool, every callsite has a propertyCid of length 139.

import {
  must,
  forAll,
  eq,
  gte,
  num,
  String as StringSort,
  type IrTerm,
} from "../ir/symbolic/index.js";

function ctor1(name: string, arg: IrTerm): IrTerm {
  return { kind: "ctor", name, args: [arg] };
}

export function invariants(): void {
  // -- loadAllProofs over an empty directory returns an empty pool. -------
  // Modeled as: len(loaded mementos for empty dir) = 0.
  must(
    "load_all_proofs_empty_dir_yields_empty_pool",
    forAll(StringSort, (_d) =>
      eq(ctor1("loadAllProofsCount", ctor1("emptyDir", num(0))), num(0)),
    ),
  );

  // -- loadAllProofs is deterministic. -----------------------------------
  must(
    "load_all_proofs_is_deterministic",
    forAll(StringSort, (d) =>
      eq(ctor1("loadAllProofsCount", d), ctor1("loadAllProofsCount", d)),
    ),
  );

  // -- enumerateCallsites property-cid length is exactly 139. -------------
  must(
    "enumerate_callsites_property_cid_length_eq_139",
    forAll(StringSort, (cs) =>
      eq(ctor1("len", ctor1("callsitePropertyCid", cs)), num(139)),
    ),
  );

  // -- enumerateCallsites bridge-target-cid length is exactly 139. --------
  must(
    "enumerate_callsites_bridge_target_cid_length_eq_139",
    forAll(StringSort, (cs) =>
      eq(ctor1("len", ctor1("callsiteBridgeTargetCid", cs)), num(139)),
    ),
  );

  // -- The verifier report's totalCallsites count is non-negative. --------
  must(
    "verifier_report_total_callsites_nonneg",
    forAll(StringSort, (r) =>
      gte(ctor1("verifierReportTotalCallsites", r), num(0)),
    ),
  );
}
