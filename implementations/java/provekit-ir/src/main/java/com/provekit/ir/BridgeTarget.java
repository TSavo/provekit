// SPDX-License-Identifier: Apache-2.0
//
// BridgeTarget: tagged-union target axis per
//   protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.R1
//
// Mirrors implementations/rust/provekit-ir-types/src/lib.rs::BridgeTarget
// (also rust's provekit_claim_envelope::BridgeTargetV14). Implementations
// MUST emit exactly one variant. Implementations MUST NOT emit a bare
// string for `target`; the substrate verifier rejects stringified
// placeholders (spec §1.R2).
//
//   { "kind": "contract",    "cid": "<contractCid>" }
//   { "kind": "contractSet", "cid": "<contractSetCid>" }

package com.provekit.ir;

import java.util.Objects;

public sealed interface BridgeTarget {

    String cid();

    String kind();

    /** Emit the canonical JSON object for this target.  JCS-canonical
     * key order is `cid` then `kind` (lexicographic by code-point). */
    default String toJson() {
        StringBuilder sb = new StringBuilder();
        sb.append("{\"cid\":\"").append(Sort.escape(cid())).append("\"");
        sb.append(",\"kind\":\"").append(Sort.escape(kind())).append("\"}");
        return sb.toString();
    }

    record Contract(String cid) implements BridgeTarget {
        public Contract {
            Objects.requireNonNull(cid, "cid");
        }
        @Override public String kind() { return "contract"; }
    }

    record ContractSet(String cid) implements BridgeTarget {
        public ContractSet {
            Objects.requireNonNull(cid, "cid");
        }
        @Override public String kind() { return "contractSet"; }
    }
}
