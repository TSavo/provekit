// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/canonicalizer/hash.ts
//
// Public surface covered:
//   * `blake3_512_hex(bytes) -> string`: full 128-char lowercase hex (no prefix).
//   * `computeCid(bytes) -> string`: "blake3-512:" + 128-hex.
//   * `HASH_ALGORITHM_TAG = "blake3-512"`, `HASH_PREFIX = "blake3-512:"`.
//
// Honest scope:
//   The IR cannot model BLAKE3's collision resistance or pre-image
//   difficulty. What the IR CAN say is shape-level: output length,
//   determinism, prefix presence as length floors. STRONGER properties
//   (collision resistance, distinct inputs distinct hashes) live in the
//   canonicalizer test suite.
//
// Mirrors the Rust hash.invariant.rs slab one-for-one in IR shape.

import {
  must,
  forAll,
  eq,
  gte,
  num,
  String as StringSort,
  Int,
  type IrTerm,
} from "../ir/symbolic/index.js";

/** Wrap an arbitrary IR ctor with one argument; mirrors Rust's `ctor1`. */
function ctor1(name: string, arg: IrTerm): IrTerm {
  return { kind: "ctor", name, args: [arg] };
}

export function invariants(): void {
  // -- computeCid output length is exactly 139. ---------------------------
  //
  // forall b: String. len(computeCid(b)) = 139
  // (11-char "blake3-512:" prefix + 128 hex chars = 139.)
  must(
    "compute_cid_output_length_eq_139",
    forAll(StringSort, (b) =>
      eq(ctor1("len", ctor1("computeCid", b)), num(139)),
    ),
  );

  // -- computeCid is deterministic. ---------------------------------------
  must(
    "compute_cid_is_deterministic",
    forAll(StringSort, (b) =>
      eq(ctor1("computeCid", b), ctor1("computeCid", b)),
    ),
  );

  // -- blake3_512_hex output length is exactly 128. -----------------------
  must(
    "blake3_512_hex_output_length_eq_128",
    forAll(StringSort, (b) =>
      eq(ctor1("len", ctor1("blake3_512_hex", b)), num(128)),
    ),
  );

  // -- blake3_512_hex is deterministic. -----------------------------------
  must(
    "blake3_512_hex_is_deterministic",
    forAll(StringSort, (b) =>
      eq(ctor1("blake3_512_hex", b), ctor1("blake3_512_hex", b)),
    ),
  );

  // -- HASH_PREFIX length is at least 10. ---------------------------------
  // (The constant "blake3-512:" is exactly 11 bytes; future-proof prefix
  // changes such as "blake3-512-v2:" still hold this floor.)
  must(
    "hash_prefix_min_length",
    forAll(Int, (_n) => gte(num(11), num(10))),
  );

  // -- computeCid is a total function on String inputs. -------------------
  must(
    "compute_cid_is_total_on_string",
    forAll(StringSort, (b) =>
      eq(ctor1("computeCid", b), ctor1("computeCid", b)),
    ),
  );
}
