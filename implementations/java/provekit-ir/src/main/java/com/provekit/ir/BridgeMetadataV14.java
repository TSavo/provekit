// SPDX-License-Identifier: Apache-2.0
//
// BridgeMetadataV14: the opaque-to-substrate portion of a v1.4
// BridgeDeclaration. All fields are optional; `null` means OMIT from
// the JCS bytes per spec §1.R2 (no `null` literal, no `pending-*` /
// `deferred:*` placeholder strings).
//
//   protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.R2
//   protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md §1
//
// Mirrors implementations/rust/provekit-ir-types/src/lib.rs::BridgeMetadataV14.

package com.provekit.ir;

public record BridgeMetadataV14(
        String targetWitnessCid,
        String targetBinaryCid,
        String targetLayer,
        String targetContractSetCid,
        String producedBy,
        String producedAt) {

    public BridgeMetadataV14 {
        if (targetWitnessCid != null) {
            BridgeTarget.requireValidCid(targetWitnessCid, "targetWitnessCid");
        }
        if (targetBinaryCid != null) {
            BridgeTarget.requireValidCid(targetBinaryCid, "targetBinaryCid");
        }
        if (targetContractSetCid != null) {
            BridgeTarget.requireValidCid(targetContractSetCid, "targetContractSetCid");
        }
    }

    /** All-absent metadata. */
    public static BridgeMetadataV14 empty() {
        return new BridgeMetadataV14(null, null, null, null, null, null);
    }
}
