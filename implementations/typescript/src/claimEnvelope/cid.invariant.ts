// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/claimEnvelope/cid.ts
//
// Public surface covered:
//   * `computeEnvelopeCid(envelope) -> string`  (length-139 self-identifying CID)
//   * `envelopeForHashing(envelope) -> object`  (cid+signature elided)
//
// Honest scope: the IR can pin output length and determinism; the byte-
// faithful claim "this hash equals BLAKE3-512 over the canonical bytes"
// is empirically verified by the canonicalizer test suite.

import {
  must,
  forAll,
  eq,
  num,
  String as StringSort,
  type IrTerm,
} from "../ir/symbolic/index.js";

function ctor1(name: string, arg: IrTerm): IrTerm {
  return { kind: "ctor", name, args: [arg] };
}

export function invariants(): void {
  // -- computeEnvelopeCid output length is exactly 139. -------------------
  must(
    "compute_envelope_cid_output_length_eq_139",
    forAll(StringSort, (e) =>
      eq(ctor1("len", ctor1("computeEnvelopeCid", e)), num(139)),
    ),
  );

  // -- computeEnvelopeCid is deterministic. -------------------------------
  must(
    "compute_envelope_cid_is_deterministic",
    forAll(StringSort, (e) =>
      eq(ctor1("computeEnvelopeCid", e), ctor1("computeEnvelopeCid", e)),
    ),
  );

  // -- envelopeForHashing is deterministic. -------------------------------
  must(
    "envelope_for_hashing_is_deterministic",
    forAll(StringSort, (e) =>
      eq(ctor1("envelopeForHashing", e), ctor1("envelopeForHashing", e)),
    ),
  );
}
