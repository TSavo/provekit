// SPDX-License-Identifier: Apache-2.0
//
// BLAKE3-512 helper. v1.1.0 of the protocol mandates self-identifying
// hashes of the form:
//
//   "blake3-512:" + lowercase-hex(64-byte-digest)
//
// Backed by the pure-Java {@link Blake3} reference implementation.
// NO truncation. v1.1.0 is scorched earth: BLAKE3-512 is the only
// hash function permitted, always 512 bits wide.

package com.provekit.canonicalizer;

import java.nio.charset.StandardCharsets;

public final class Hash {

    private Hash() {}

    public static final String BLAKE3_512_PREFIX = "blake3-512:";

    /**
     * Returns the spec's self-identifying string form:
     * {@code "blake3-512:" + lowercase-hex(64-byte digest)}.
     */
    public static String blake3_512(byte[] input) {
        if (input == null) throw new NullPointerException("input");
        return Blake3.blake3_512Cid(input);
    }

    /** Convenience: hash a UTF-8 string. */
    public static String blake3_512Utf8(String s) {
        return blake3_512(s.getBytes(StandardCharsets.UTF_8));
    }
}
