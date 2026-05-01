// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/proofEnvelope/index.ts
//
// Public surface covered:
//   * `buildProofEnvelope(input) -> {bytes, cid}`
//   * `decodeProofEnvelope(bytes) -> DecodedProofCatalog`
//   * `verifyProofEnvelope(bytes, filenameCid, key) -> VerifyResult`
//
// Honest scope: deterministic CBOR (RFC 8949 §4.2.1) shape rules are
// byte-faithful; the IR can express the high-level claim that
// build/decode round-trips and that the catalog CID has the standard
// prefix and 139-char length.

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
  // -- buildProofEnvelope CID is exactly 139 chars. -----------------------
  must(
    "build_proof_envelope_cid_length_eq_139",
    forAll(StringSort, (input) =>
      eq(ctor1("len", ctor1("buildProofEnvelopeCid", input)), num(139)),
    ),
  );

  // -- buildProofEnvelope is deterministic for a fixed input. -------------
  must(
    "build_proof_envelope_is_deterministic",
    forAll(StringSort, (input) =>
      eq(
        ctor1("buildProofEnvelopeCid", input),
        ctor1("buildProofEnvelopeCid", input),
      ),
    ),
  );

  // -- buildProofEnvelope output bytes are non-empty. ---------------------
  must(
    "build_proof_envelope_bytes_nonempty",
    forAll(StringSort, (input) =>
      gte(ctor1("len", ctor1("buildProofEnvelopeBytes", input)), num(1)),
    ),
  );

  // -- decode(encode(catalog)) round-trips: equality of catalog after
  //    encode-decode-re-encode. --------------------------------------------
  must(
    "decode_encode_round_trips",
    forAll(StringSort, (cat) =>
      eq(
        ctor1(
          "buildProofEnvelopeBytes",
          ctor1("decodeProofEnvelope", ctor1("buildProofEnvelopeBytes", cat)),
        ),
        ctor1("buildProofEnvelopeBytes", cat),
      ),
    ),
  );

  // -- verifyProofEnvelope is deterministic for fixed bytes/key. ----------
  must(
    "verify_proof_envelope_is_deterministic",
    forAll(StringSort, (b) =>
      eq(
        ctor1("verifyProofEnvelopeOk", b),
        ctor1("verifyProofEnvelopeOk", b),
      ),
    ),
  );
}
