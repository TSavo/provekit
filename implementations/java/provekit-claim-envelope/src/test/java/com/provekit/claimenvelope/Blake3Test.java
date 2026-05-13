// SPDX-License-Identifier: Apache-2.0

package com.provekit.claimenvelope;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

import com.provekit.ir.Blake3;

class Blake3Test {

    @Test
    void empty_input_known_vector() {
        // Pinned BLAKE3 (extended to 64 bytes) of the empty input. Known
        // test vector from the BLAKE3 reference test suite. Confirms the
        // BouncyCastle Blake3Digest output matches the canonical spec.
        String cid = Blake3.blake3_512(new byte[0]);
        assertEquals(
            "blake3-512:" +
                "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262" +
                "e00f03e7b69af26b7faaf09fcd333050338ddfe085b8cc869ca98b206c08243a",
            cid);
    }

    @Test
    void prefix_and_length() {
        String cid = Blake3.blake3_512("hello".getBytes());
        assertTrue(cid.startsWith("blake3-512:"));
        // 11-char prefix + 128 hex digits
        assertEquals(11 + 128, cid.length());
    }
}
