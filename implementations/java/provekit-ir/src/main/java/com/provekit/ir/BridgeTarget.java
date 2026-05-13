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
import java.util.regex.Pattern;

public sealed interface BridgeTarget {

    /** Canonical CID grammar for bridge fields (spec §1.R2).
     *  Rejects placeholder strings (pending-*:, deferred:*) and any
     *  value that is not a well-formed blake3-512 content identifier. */
    static final Pattern VALID_CID = Pattern.compile(
            "^blake3-512:[0-9a-f]{128}$");

    /** Validate that the given value is a well-formed CID and is not a
     *  placeholder string.  Throws {@link IllegalArgumentException} with
     *  a message naming the offending field + value if validation fails. */
    static String requireValidCid(String value, String fieldName) {
        Objects.requireNonNull(value, fieldName);
        if (value.isEmpty()) {
            throw new IllegalArgumentException(
                    fieldName + " must not be empty; got empty string");
        }
        if (value.startsWith("pending-")) {
            throw new IllegalArgumentException(
                    fieldName + " must not be a placeholder string (pending-*:); got: " + value);
        }
        if (value.startsWith("deferred:")) {
            throw new IllegalArgumentException(
                    fieldName + " must not be a placeholder string (deferred:*); got: " + value);
        }
        if (!VALID_CID.matcher(value).matches()) {
            throw new IllegalArgumentException(
                    fieldName + " must match canonical CID grammar ^blake3-512:[0-9a-f]{128}$; got: " + value);
        }
        return value;
    }

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
            requireValidCid(cid, "cid");
        }
        @Override public String kind() { return "contract"; }
    }

    record ContractSet(String cid) implements BridgeTarget {
        public ContractSet {
            requireValidCid(cid, "cid");
        }
        @Override public String kind() { return "contractSet"; }
    }
}
