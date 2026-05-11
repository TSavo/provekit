package com.provekit.ir;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.charset.StandardCharsets;
import java.util.List;
import org.junit.jupiter.api.Test;

class CanonicalJsonTest {
    @Test
    void canonicalJsonSortsObjectKeysAndEscapesControlCharacters() {
        Jcs.Json value = Jcs.object(
            "b", Jcs.integer(2),
            "a", Jcs.string("x\n\"y\"")
        );

        assertEquals("{\"a\":\"x\\u000a\\\"y\\\"\",\"b\":2}", Jcs.encode(value));
    }

    @Test
    void blake3_512MatchesKnownVectors() {
        assertEquals(
            "blake3-512:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262e00f03e7b69af26b7faaf09fcd333050338ddfe085b8cc869ca98b206c08243a",
            Jcs.blake3_512(new byte[0])
        );
        assertEquals(
            "blake3-512:6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d851fb250ae7393f5d02813b65d521a0d492d9ba09cf7ce7f4cffd900f23374bf0b",
            Jcs.blake3_512("abc".getBytes(StandardCharsets.UTF_8))
        );
    }

    @Test
    void blake3_512HandlesMultiChunkInput() {
        byte[] input = "a".repeat(2000).getBytes(StandardCharsets.UTF_8);
        assertEquals(
            "blake3-512:c849401fee5e93cf05cbbeb2c60ae6fda8778dfadb996910aded82052fff769d69848252aaa7cb55790de15e20ae5e09234b29050fd82886aefdd8f0b0c9a011",
            Jcs.blake3_512(input)
        );
    }

    @Test
    void cidHashesCanonicalJsonBytes() {
        Jcs.Json left = Jcs.object("b", Jcs.integer(2), "a", Jcs.integer(1));
        Jcs.Json right = Jcs.object("a", Jcs.integer(1), "b", Jcs.integer(2));

        assertEquals(Jcs.cid(left), Jcs.cid(right));
        assertTrue(Jcs.cid(left).startsWith("blake3-512:"));
        assertEquals("blake3-512:".length() + 128, Jcs.cid(left).length());
    }

    @Test
    void parserRoundTripsGeneratedCanonicalJson() {
        Jcs.Json value = Jcs.object(
            "xs", Jcs.array(List.of(Jcs.integer(1), Jcs.bool(true), Jcs.nullValue())),
            "name", Jcs.string("C.f(int)")
        );

        assertEquals(Jcs.encode(value), Jcs.encode(Jcs.parse(Jcs.encode(value))));
    }
}
