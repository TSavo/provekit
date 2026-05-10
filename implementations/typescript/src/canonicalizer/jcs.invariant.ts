// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/claimEnvelope/canonicalize.ts
// (the JCS encoder for claim envelopes: RFC 8785).
//
// Public surface covered:
//   * `canonicalEncode(value) -> Buffer`: UTF-8 bytes of canonical JSON.
//   * `canonicalJsonString(value) -> string`: sorted-keys, no whitespace.
//
// Honest scope: the IR cannot model RFC 8785 §3.2.3 key-order at the
// byte-shape level. It CAN say: encode is a function (deterministic),
// output is non-empty for any well-formed value, the empty-array
// emission is exactly "[]" length 2, etc.

import {
  must,
  contract,
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
  // -- canonicalEncode is a function: same input, same output. ------------
  must(
    "canonical_encode_is_deterministic",
    forAll(StringSort, (s) =>
      eq(ctor1("canonicalEncode", s), ctor1("canonicalEncode", s)),
    ),
  );

  // -- canonicalEncode output length >= 1 for any value. ------------------
  must(
    "canonical_encode_output_nonempty",
    forAll(StringSort, (v) =>
      gte(ctor1("len", ctor1("canonicalEncode", v)), num(1)),
    ),
  );

  // -- canonicalJsonString is deterministic. ------------------------------
  must(
    "canonical_json_string_is_deterministic",
    forAll(StringSort, (s) =>
      eq(ctor1("canonicalJsonString", s), ctor1("canonicalJsonString", s)),
    ),
  );

  // -- "true" emission is exactly "true", length 4 (post-only contract). --
  contract("canonical_encode_true_length_eq_4", {
    post: eq(
      ctor1("len", ctor1("canonicalJsonString", ctor1("trueValue", num(0)))),
      num(4),
    ),
  });

  // -- Empty-array emission is exactly "[]", length 2. --------------------
  contract("canonical_encode_empty_array_length_eq_2", {
    post: eq(
      ctor1(
        "len",
        ctor1("canonicalJsonString", ctor1("emptyArrayValue", num(0))),
      ),
      num(2),
    ),
  });

  // -- Empty-object emission is exactly "{}", length 2. -------------------
  contract("canonical_encode_empty_object_length_eq_2", {
    post: eq(
      ctor1(
        "len",
        ctor1("canonicalJsonString", ctor1("emptyObjectValue", num(0))),
      ),
      num(2),
    ),
  });

  // -- canonicalJsonString of null is exactly "null", length 4. -----------
  contract("canonical_encode_null_length_eq_4", {
    post: eq(
      ctor1("len", ctor1("canonicalJsonString", ctor1("nullValue", num(0)))),
      num(4),
    ),
  });
}
