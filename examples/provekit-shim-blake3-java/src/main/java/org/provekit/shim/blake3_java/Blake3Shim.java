// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-blake3-java: Bouncy Castle's @ProveKitSugar shim.
//
// Realizes concept:family:hash concepts via org.bouncycastle.crypto.digests.Blake3Digest.
// Sister shim to provekit-shim-blake3-rust (crate `blake3`). Both members of
// concept:family:hash; concept names aligned 1:1.

package org.provekit.shim.blake3_java;

import com.provekit.lift.java_source.ProveKitSugar;
import org.bouncycastle.crypto.digests.Blake3Digest;

/**
 * Java realizations of concept:family:hash via Bouncy Castle.
 */
public final class Blake3Shim {

    private Blake3Shim() {
        // Utility class.
    }

    /**
     * {@code concept:blake3-512-of} — compute the BLAKE3-512 hash of the input bytes.
     * Mirrors {@code provekit-shim-blake3-rust::blake3_512_of(bytes: &[u8]) -> [u8; 64]}.
     *
     * Bouncy Castle's Blake3Digest defaults to 32 bytes; this overload requests
     * 512 bits (64 bytes) to match the substrate's BLAKE3-512 contract.
     */
    @ProveKitSugar(
        concept = "concept:blake3-512-of",
        library = "bouncycastle",
        family = "concept:family:hash",
        version = "1.78",
        loss = {}
    )
    public static byte[] blake3_512_of(byte[] bytes) {
        Blake3Digest digest = new Blake3Digest(512);  // 512 bits = 64-byte output
        digest.update(bytes, 0, bytes.length);
        byte[] out = new byte[64];
        digest.doFinal(out, 0);
        return out;
    }
}
