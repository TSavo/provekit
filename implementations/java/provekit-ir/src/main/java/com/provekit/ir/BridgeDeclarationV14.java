// SPDX-License-Identifier: Apache-2.0
//
// BridgeDeclarationV14: the v1.4 layered envelope/header/body shape.
//
//   { "envelope": {...}, "header": {...}, "metadata": {...} }
//
// Source of truth:
//   protocol/specs/2026-05-03-bridge-target-dimensionality.md
//   protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md
//   protocol/provekit-ir.cddl  BridgeDeclarationV14
//
// Mirrors implementations/rust/provekit-ir-types/src/lib.rs::BridgeDeclarationV14.
//
// Wire emission lives in
// implementations/java/provekit-claim-envelope/.../ClaimEnvelope#mintBridgeV14.
// This record is the typed in-memory shape; canonical JCS encoding is
// performed via the Jcs encoder in the claim-envelope module.

package com.provekit.ir;

import java.util.Objects;

public record BridgeDeclarationV14(
        BridgeEnvelope envelope,
        BridgeHeaderV14 header,
        BridgeMetadataV14 metadata) {

    public BridgeDeclarationV14 {
        Objects.requireNonNull(envelope, "envelope");
        Objects.requireNonNull(header, "header");
        Objects.requireNonNull(metadata, "metadata");
    }
}
