// SPDX-License-Identifier: Apache-2.0
//
// Cross-language conformance tests for the Java JCS encoder. Mirrors
// the python peer's test_canonicalizer.py byte-for-byte. The protocol
// IS the bytes; if any pin here drifts, the Java impl is wrong.

package com.provekit.canonicalizer;

import org.junit.jupiter.api.Test;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

class JcsTest {

    @Test
    void emptyObjectAndArray() {
        assertEquals("{}", Jcs.encode(Value.ofObject(List.of())));
        assertEquals("[]", Jcs.encode(Value.ofArray(List.of())));
    }

    @Test
    void objectKeysSortByCodepoint() {
        Value v = Value.ofObject(List.of(
            Map.entry("b", Value.ofInt(1)),
            Map.entry("a", Value.ofString("x"))));
        assertEquals("{\"a\":\"x\",\"b\":1}", Jcs.encode(v));
    }

    @Test
    void stringEscapesQuoteAndBackslash() {
        assertEquals("\"a\\\"b\\\\c\"", Jcs.encode(Value.ofString("a\"b\\c")));
    }

    @Test
    void controlCharLowerHexEscape() {
        assertEquals("\"\\u0001\"",
            Jcs.encode(Value.ofString(String.valueOf((char) 0x01))));
        assertEquals("\"\\u001f\"",
            Jcs.encode(Value.ofString(String.valueOf((char) 0x1F))));
        // U+0020 (space) is NOT escaped per RFC 8785.
        assertEquals("\" \"", Jcs.encode(Value.ofString(" ")));
    }

    @Test
    void unicodeAtomicPredicateGlyphsRoundTripVerbatim() {
        // The kit's atomic predicate names use glyphs above U+0080.
        // Cross-language hash agreement requires UTF-8 verbatim emission.
        for (String sym : new String[] { "≥", "≤", "≠" }) { // >= <= !=
            String encoded = Jcs.encode(Value.ofString(sym));
            assertEquals("\"" + sym + "\"", encoded);
            // Inner bytes are the same UTF-8 the input carried.
            String inner = encoded.substring(1, encoded.length() - 1);
            assertArrayEquals(sym.getBytes(StandardCharsets.UTF_8),
                inner.getBytes(StandardCharsets.UTF_8));
        }
    }

    @Test
    void unicodeInObjectKeyAndValueMatchesRust() {
        Value v = Value.ofObject(List.of(
            Map.entry("name", Value.ofString("≥"))));
        String encoded = Jcs.encode(v);
        assertEquals("{\"name\":\"≥\"}", encoded);
        // Byte-identical to the rust impl: e2 89 a5 for >=.
        assertArrayEquals(
            new byte[] { 0x7B, 0x22, 0x6E, 0x61, 0x6D, 0x65, 0x22, 0x3A, 0x22,
                         (byte) 0xE2, (byte) 0x89, (byte) 0xA5, 0x22, 0x7D },
            encoded.getBytes(StandardCharsets.UTF_8));
    }

    @Test
    void boolAndNullEmitVerbatim() {
        assertEquals("true", Jcs.encode(Value.ofBool(true)));
        assertEquals("false", Jcs.encode(Value.ofBool(false)));
        assertEquals("null", Jcs.encode(Value.ofNull()));
    }

    @Test
    void integerEncoding() {
        assertEquals("0", Jcs.encode(Value.ofInt(0)));
        assertEquals("42", Jcs.encode(Value.ofInt(42)));
        assertEquals("-7", Jcs.encode(Value.ofInt(-7)));
        assertEquals("9223372036854775807", Jcs.encode(Value.ofInt(Long.MAX_VALUE)));
    }

    @Test
    void encodeUtf8MatchesEncodeBytes() {
        Value v = Value.ofObject(List.of(
            Map.entry("a", Value.ofString("x")),
            Map.entry("b", Value.ofArray(List.of(Value.ofInt(1), Value.ofBool(true))))));
        byte[] direct = Jcs.encodeUtf8(v);
        byte[] viaString = Jcs.encode(v).getBytes(StandardCharsets.UTF_8);
        assertArrayEquals(viaString, direct);
    }

    @Test
    void jcsHashSelfIdentifyingPrefix() {
        String h = Hash.blake3_512(Jcs.encodeUtf8(Value.ofString("hi")));
        assertTrue(h.startsWith(Hash.BLAKE3_512_PREFIX));
        assertEquals(Hash.BLAKE3_512_PREFIX.length() + 128, h.length());
    }

    @Test
    void nestedObjectKeysSortAtEachLevel() {
        Value inner = Value.ofObject(List.of(
            Map.entry("z", Value.ofInt(1)),
            Map.entry("a", Value.ofInt(2))));
        Value v = Value.ofObject(List.of(
            Map.entry("y", inner),
            Map.entry("x", Value.ofInt(3))));
        assertEquals("{\"x\":3,\"y\":{\"a\":2,\"z\":1}}", Jcs.encode(v));
    }
}
