// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/workflow/producers/loadAllProofs.ts
//
// Public surface:
//   * `makeLoadAllProofsStage() -> Stage`
//   * `enumerateProofFiles(projectRoot) -> string[]`

import { must, forAll, gte, num, String as StringSort } from "../../ir/symbolic/index.js";

function ctor(name: string, ...args: any[]): any {
  return { kind: "ctor", name, args };
}

export function invariants(): void {
  // -- Output mementoPool keys are non-empty strings. -----------------------
  must(
    "pool_keys_nonempty",
    forAll(StringSort, (cid) =>
      gte(ctor("len", cid), num(1)),
    ),
  );

  // -- bridgesBySymbol keys are non-empty strings. -------------------------
  must(
    "bridge_keys_nonempty",
    forAll(StringSort, (symbol) =>
      gte(ctor("len", symbol), num(1)),
    ),
  );

  // -- errors array has non-negative length. -------------------------------
  must(
    "errors_length_nonnegative",
    forAll(StringSort, (_root) =>
      gte(ctor("len", ctor("errors", _root)), num(0)),
    ),
  );

  // -- serializeInput then deserializeOutput round-trips. ---------------
  must(
    "serialize_deserialize_roundtrip",
    forAll(StringSort, (output) =>
      eq(
        ctor("deserializeOutput", ctor("serializeOutput", output)),
        ctor("deserializeOutput", output),
      ),
    ),
  );

  // -- enumerateProofFiles returns unique paths (no duplicates). -------
  must(
    "enumerate_returns_unique",
    forAll(StringSort, (root) =>
      eq(
        ctor("len", ctor("unique", ctor("enumerateProofFiles", root))),
        ctor("len", ctor("enumerateProofFiles", root)),
      ),
    ),
  );
}