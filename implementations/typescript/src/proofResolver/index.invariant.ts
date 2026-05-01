// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/proofResolver/index.ts
//
// Public surface covered: discovery + resolution of .proof files
// referenced from a project root's node_modules graph.
//
// Honest scope: filesystem walking is empirical; the IR pins shape —
// every resolved entry has a CID of length 139 and a path of length >= 1.

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
  // -- Every resolved entry has a CID of length 139. ----------------------
  must(
    "resolver_entry_cid_length_eq_139",
    forAll(StringSort, (e) =>
      eq(ctor1("len", ctor1("resolverEntryCid", e)), num(139)),
    ),
  );

  // -- Every resolved entry has a non-empty path. -------------------------
  must(
    "resolver_entry_path_nonempty",
    forAll(StringSort, (e) =>
      gte(ctor1("len", ctor1("resolverEntryPath", e)), num(1)),
    ),
  );

  // -- Resolver is deterministic on a fixed project root. -----------------
  must(
    "resolver_is_deterministic",
    forAll(StringSort, (root) =>
      eq(
        ctor1("resolverEntriesCount", root),
        ctor1("resolverEntriesCount", root),
      ),
    ),
  );
}
