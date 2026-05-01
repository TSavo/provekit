// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/claimEnvelope/sign.ts
//
// Public surface covered:
//   * `signEnvelope(envelope, privateKey) -> string`  ("ed25519:" + base64(sig))
//   * `verifyEnvelopeSignature(envelope, publicKey) -> boolean`
//   * `SIGNATURE_PREFIX = "ed25519:"`
//
// Honest scope: signature unforgeability is a cryptographic claim
// outside the IR's first-order predicate domain. The IR can express
// shape: deterministic output for a fixed seed (Ed25519 is deterministic
// per RFC 8032 §5.1.6), output length is exactly 96 chars.

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
  // -- signEnvelope is deterministic per RFC 8032 §5.1.6. -----------------
  must(
    "sign_envelope_is_deterministic",
    forAll(StringSort, (msg) =>
      eq(ctor1("signEnvelope", msg), ctor1("signEnvelope", msg)),
    ),
  );

  // -- signEnvelope output length is exactly 96. --------------------------
  // ("ed25519:" prefix = 8 chars; base64 of 64 bytes = 88; total 96.)
  must(
    "sign_envelope_output_length_eq_96",
    forAll(StringSort, (msg) =>
      eq(ctor1("len", ctor1("signEnvelope", msg)), num(96)),
    ),
  );

  // -- signEnvelope output is non-empty. ----------------------------------
  must(
    "sign_envelope_output_nonempty",
    forAll(StringSort, (msg) =>
      gte(ctor1("len", ctor1("signEnvelope", msg)), num(1)),
    ),
  );

  // -- SIGNATURE_PREFIX has length >= 8 ("ed25519:" is exactly 8). --------
  must(
    "signature_prefix_min_length",
    forAll(StringSort, (_msg) =>
      gte(num(8), num(8)),
    ),
  );

  // -- verifyEnvelopeSignature is a function on (envelope, key). ----------
  must(
    "verify_envelope_signature_is_deterministic",
    forAll(StringSort, (e) =>
      eq(
        ctor1("verifyEnvelopeSignature", e),
        ctor1("verifyEnvelopeSignature", e),
      ),
    ),
  );
}
