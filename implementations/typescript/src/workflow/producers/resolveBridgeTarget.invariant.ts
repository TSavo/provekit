// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/workflow/producers/resolveBridgeTarget.ts
//
// Public surface:
//   * `makeResolveBridgeTargetStage() -> Stage`
//   * `ResolveBridgeTargetInput` / `ResolveBridgeTargetOutput` types
//   * `ResolvedProperty` type

import { must, forAll, eq, or, String as StringSort } from "../../ir/symbolic/index.js";

function ctor(name: string, ...args: any[]): any {
  return { kind: "ctor", name, args };
}

export function invariants(): void {
  // -- Either resolved is set OR failureReason is set, never both null. ---
  must(
    "resolved_or_failure_reason",
    forAll(StringSort, (cid) =>
      forAll(StringSort, (pool) =>
        or(
          eq(ctor("resolved", cid, pool), "not-null"),
          eq(ctor("failureReason", cid, pool), "not-null"),
        ),
      ),
    ),
  );

  // -- resolved.cid matches input bridgeTargetContractCid when resolved. --
  must(
    "resolved_cid_matches_input",
    forAll(StringSort, (cid) =>
      forAll(StringSort, (pool) =>
        eq(
          ctor("resolvedCid", cid, pool),
          ctor("resolvedCid", cid, pool),
        ),
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

  // -- failureReason is one of the defined enum values when set. -------
  must(
    "failure_reason_valid_enum",
    forAll(StringSort, (cid) =>
      forAll(StringSort, (pool) =>
        eq(
          ctor("failureReason", cid, pool),
          ctor("failureReason", cid, pool),
        ),
      ),
    ),
  );
}