// SPDX-License-Identifier: Apache-2.0
//
// BridgeEnvelope: signer / declaredAt / signature triple, per
//   protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md §1, §2
//
// Mirrors implementations/rust/provekit-ir-types/src/lib.rs::BridgeEnvelope.
// The envelope's signature is computed over JCS({header, metadata}) and
// the envelope's CID (the attestation CID) is BLAKE3-512(JCS(envelope))
// AFTER the signature has been embedded.

package com.provekit.ir;

import java.util.Objects;

public record BridgeEnvelope(String signer, String declaredAt, String signature) {

    public BridgeEnvelope {
        Objects.requireNonNull(signer, "signer");
        Objects.requireNonNull(declaredAt, "declaredAt");
        Objects.requireNonNull(signature, "signature");
    }
}
