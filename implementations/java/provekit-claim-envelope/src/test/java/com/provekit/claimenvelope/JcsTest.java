// SPDX-License-Identifier: Apache-2.0

package com.provekit.claimenvelope;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;

import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;
import java.util.List;

import org.junit.jupiter.api.Test;

import com.provekit.claimenvelope.Jcs.Value;

/**
 * Unit tests mirroring implementations/rust/provekit-canonicalizer/src/jcs.rs.
 */
class JcsTest {

    @Test
    void encode_simple_object_sorts_keys() {
        Value v = Value.object("b", Value.integer(1), "a", Value.string("x"));
        assertEquals("{\"a\":\"x\",\"b\":1}", Jcs.encode(v));
    }

    @Test
    void encode_nested_array_object() {
        Value v = Value.object("xs", Value.array(Value.integer(1), Value.integer(2)));
        assertEquals("{\"xs\":[1,2]}", Jcs.encode(v));
    }

    @Test
    void escape_quotes_and_backslash() {
        Value v = Value.string("a\"b\\c");
        assertEquals("\"a\\\"b\\\\c\"", Jcs.encode(v));
    }

    @Test
    void empty_object_and_array() {
        assertEquals("{}", Jcs.encode(Value.object(new LinkedHashMap<>())));
        assertEquals("[]", Jcs.encode(Value.array(List.of())));
    }

    @Test
    void unicode_atomic_predicates_round_trip_verbatim() {
        // U+2265 = >=, U+2264 = <=, U+2260 = !=. The kit's atomic predicate
        // names use exactly these. Cross-language hash agreement depends on
        // these chars round-tripping verbatim (UTF-8 bytes preserved).
        for (String sym : new String[]{"≥", "≤", "≠"}) {
            Value v = Value.string(sym);
            String encoded = Jcs.encode(v);
            assertEquals("\"" + sym + "\"", encoded);
            String inner = encoded.substring(1, encoded.length() - 1);
            assertArrayEquals(sym.getBytes(StandardCharsets.UTF_8), inner.getBytes(StandardCharsets.UTF_8));
        }
    }

    @Test
    void mixed_ascii_and_unicode_preserved() {
        String s = "x ≥ 0";
        Value v = Value.string(s);
        String encoded = Jcs.encode(v);
        assertEquals("\"" + s + "\"", encoded);
    }

    @Test
    void unicode_in_object_key_and_value_matches_cpp_bytes() {
        Value v = Value.object("name", Value.string("≥"));
        byte[] encoded = Jcs.encodeUtf8(v);
        // The cpp peer writes raw UTF-8 bytes for non-ASCII chars; we must match.
        byte[] expected = new byte[]{
            '{','"','n','a','m','e','"',':','"',
            (byte) 0xe2, (byte) 0x89, (byte) 0xa5,
            '"','}'
        };
        assertArrayEquals(expected, encoded);
    }

    @Test
    void control_char_escapes_lowercase_hex() {
        Value v = Value.string("\t\n");
        assertEquals("\"\\u0009\\u000a\"", Jcs.encode(v));
    }

    @Test
    void integer_zero_serializes_as_zero() {
        assertEquals("0", Jcs.encode(Value.integer(0)));
    }

    @Test
    void integer_negative_serializes_with_minus() {
        assertEquals("-42", Jcs.encode(Value.integer(-42)));
    }
}
