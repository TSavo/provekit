// SPDX-License-Identifier: Apache-2.0
//
// Cross-language conformance tests for the deterministic CBOR encoder.
// Pinned head encodings come from RFC 8949 §4.2.1 examples and from
// the rust peer's exact byte expectations.

package com.provekit.proofenvelope;

import org.junit.jupiter.api.Test;
import java.io.ByteArrayOutputStream;
import static org.junit.jupiter.api.Assertions.*;

class CborTest {

    private static byte[] head(int major, long arg) {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        Cbor.appendHead(out, major, arg);
        return out.toByteArray();
    }

    @Test
    void shortestFormImmediateForSmallArg() {
        // major=0 (unsigned), arg=23 -> 0x17 (immediate).
        assertArrayEquals(new byte[] { 0x17 }, head(0, 23));
        // major=5 (map), arg=7 -> 0xA7.
        assertArrayEquals(new byte[] { (byte) 0xA7 }, head(5, 7));
    }

    @Test
    void shortestFormUint8() {
        // major=0, arg=24 -> 0x18, 0x18.
        assertArrayEquals(new byte[] { 0x18, 0x18 }, head(0, 24));
        // major=0, arg=255 -> 0x18, 0xFF.
        assertArrayEquals(new byte[] { 0x18, (byte) 0xFF }, head(0, 255));
    }

    @Test
    void shortestFormUint16() {
        // major=0, arg=256 -> 0x19, 0x01, 0x00.
        assertArrayEquals(new byte[] { 0x19, 0x01, 0x00 }, head(0, 256));
        // major=2 (bstr), arg=65535 -> 0x59, 0xFF, 0xFF.
        assertArrayEquals(new byte[] { 0x59, (byte) 0xFF, (byte) 0xFF }, head(2, 65535));
    }

    @Test
    void shortestFormUint32() {
        // major=0, arg=65536 -> 0x1A, 0x00, 0x01, 0x00, 0x00.
        assertArrayEquals(new byte[] { 0x1A, 0x00, 0x01, 0x00, 0x00 }, head(0, 65536));
    }

    @Test
    void encodeBstrWritesHeadThenBytes() {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        Cbor.encodeBstr(out, new byte[] { 0x01, 0x02, 0x03 });
        // major=2, arg=3 -> 0x43, then payload 0x01 0x02 0x03.
        assertArrayEquals(new byte[] { 0x43, 0x01, 0x02, 0x03 }, out.toByteArray());
    }

    @Test
    void encodeTstrUtf8() {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        Cbor.encodeTstr(out, "hi");
        // major=3, arg=2 -> 0x62, then "hi".
        assertArrayEquals(new byte[] { 0x62, 0x68, 0x69 }, out.toByteArray());
    }

    @Test
    void encodeTstrNonAscii() {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        Cbor.encodeTstr(out, "≥"); // U+2265, UTF-8 = 0xE2 0x89 0xA5
        // major=3, arg=3 -> 0x63, then UTF-8 bytes.
        assertArrayEquals(
            new byte[] { 0x63, (byte) 0xE2, (byte) 0x89, (byte) 0xA5 },
            out.toByteArray());
    }

    @Test
    void mapHeadCount() {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        Cbor.encodeMapHead(out, 7);
        assertArrayEquals(new byte[] { (byte) 0xA7 }, out.toByteArray());
    }
}
