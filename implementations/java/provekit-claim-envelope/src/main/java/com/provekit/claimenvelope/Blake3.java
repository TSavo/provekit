// SPDX-License-Identifier: Apache-2.0
//
// BLAKE3-512 hashing for the ProvekIt substrate. BLAKE3 has an
// extendable output; the protocol fixes it at 64 bytes (512 bits)
// and stamps the self-identifying prefix "blake3-512:".
//
// Mirrors implementations/csharp/Provekit.Canonicalizer/Hash.cs and
// implementations/rust/provekit-canonicalizer/src/hash.rs 1:1.

package com.provekit.claimenvelope;

import org.bouncycastle.crypto.digests.Blake3Digest;

public final class Blake3 {
    public static final String PREFIX = "blake3-512:";
    public static final int DIGEST_BITS = 512;
    public static final int DIGEST_BYTES = DIGEST_BITS / 8;

    private Blake3() {}

    /** Raw 64-byte BLAKE3 digest of {@code input}. */
    public static byte[] digest(byte[] input) {
        Blake3Digest d = new Blake3Digest(DIGEST_BITS);
        d.update(input, 0, input.length);
        byte[] out = new byte[DIGEST_BYTES];
        d.doFinal(out, 0, DIGEST_BYTES);
        return out;
    }

    /** Self-identifying CID string: "blake3-512:" + lowercase hex of the 64-byte digest. */
    public static String blake3_512(byte[] input) {
        byte[] d = digest(input);
        StringBuilder sb = new StringBuilder(PREFIX.length() + d.length * 2);
        sb.append(PREFIX);
        for (byte b : d) {
            sb.append(HEX[(b >>> 4) & 0xF]);
            sb.append(HEX[b & 0xF]);
        }
        return sb.toString();
    }

    private static final char[] HEX = {
        '0','1','2','3','4','5','6','7','8','9','a','b','c','d','e','f'
    };
}
