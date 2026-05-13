// SPDX-License-Identifier: Apache-2.0
//
// BridgeHeaderV14: the substrate-load-bearing portion of a v1.4
// BridgeDeclaration, carrying the contract-axis claim only, per
//   protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.R3
//   protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md §1
//
// Mirrors implementations/rust/provekit-ir-types/src/lib.rs::BridgeHeaderV14.
// The seven canonical fields are: schemaVersion, kind, name,
// sourceSymbol, sourceLayer, sourceContractCid, target. `schemaVersion`
// is the literal string "1" (the v1.4-layered schema version, distinct
// from the v1.2 layered shape's "2"). `kind` is the literal "bridge".

package com.provekit.ir;

import java.util.Objects;

public record BridgeHeaderV14(
        String schemaVersion,
        String kind,
        String name,
        String sourceSymbol,
        String sourceLayer,
        String sourceContractCid,
        BridgeTarget target) {

    /** Schema version stamp on v1.4 layered bridge headers. */
    public static final String SCHEMA_VERSION = "1";

    /** Literal `kind` discriminator for bridge declarations. */
    public static final String KIND = "bridge";

    public BridgeHeaderV14 {
        Objects.requireNonNull(schemaVersion, "schemaVersion");
        Objects.requireNonNull(kind, "kind");
        Objects.requireNonNull(name, "name");
        Objects.requireNonNull(sourceSymbol, "sourceSymbol");
        Objects.requireNonNull(sourceLayer, "sourceLayer");
        Objects.requireNonNull(target, "target");
        BridgeTarget.requireValidCid(sourceContractCid, "sourceContractCid");
    }

    /** Convenience factory: schemaVersion="1", kind="bridge". */
    public static BridgeHeaderV14 of(
            String name,
            String sourceSymbol,
            String sourceLayer,
            String sourceContractCid,
            BridgeTarget target) {
        return new BridgeHeaderV14(SCHEMA_VERSION, KIND, name, sourceSymbol,
                sourceLayer, sourceContractCid, target);
    }
}
