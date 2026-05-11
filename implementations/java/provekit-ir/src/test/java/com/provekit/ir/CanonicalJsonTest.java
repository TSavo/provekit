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
    void rustCanonicalizerVectorsMatchPinnedBytesAndCids() {
        assertEquals(
            "blake3-512:2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213"
                + "c3a6cb8bf623e20cdb535f8d1a5ffb86342d9c0b64aca3bce1d31f60adfa137b",
            Jcs.blake3_512(vectorInput(1))
        );
        assertEquals(
            "blake3-512:7b7015bb92cf0b318037702a6cdd81dee41224f734684c2c122cd6359cb1ee63"
                + "d8386b22e2ddc05836b7c1bb693d92af006deb5ffbc4c70fb44d0195d0c6f252",
            Jcs.blake3_512(vectorInput(2))
        );
        assertEquals(
            "blake3-512:3f8770f387faad08faa9d8414e9f449ac68e6ff0417f673f602a646a891419fe"
                + "66036ef6e6d1a8f54baa9fed1fc11c77cfb9cff65bae915045027046ebe0c01b",
            Jcs.blake3_512(vectorInput(7))
        );

        assertRustVector(
            Jcs.object(
                "\uD800\uDC00", Jcs.integer(5),
                "z", Jcs.integer(2),
                "\u00e9", Jcs.integer(3),
                "a", Jcs.integer(1),
                "\uE000", Jcs.integer(4)
            ),
            "{\"a\":1,\"z\":2,\"\u00e9\":3,\"\uE000\":4,\"\uD800\uDC00\":5}",
            "blake3-512:518cc00f6c944d8f5279b35cebf0d99752e19ddea3bbefb6e53931c4a7a261f2"
                + "bf269a022b418ee5411a0dcc266b7086842652ba2ecf16c423bef9553a886695"
        );

        assertRustVector(
            Jcs.object(
                "nil", Jcs.nullValue(),
                "int", Jcs.integer(-42),
                "array", Jcs.array(
                    Jcs.object("b", Jcs.integer(2), "a", Jcs.integer(1)),
                    Jcs.object("y", Jcs.integer(20))
                ),
                "unicode", Jcs.string("h\u00e9llo \u2265 \u65e5\u672c\u8a9e"),
                "esc", Jcs.string("quote \" slash / backslash \\ newline\n"),
                "bool", Jcs.bool(true)
            ),
            "{\"array\":[{\"a\":1,\"b\":2},{\"y\":20}],\"bool\":true,"
                + "\"esc\":\"quote \\\" slash / backslash \\\\ newline\\u000a\","
                + "\"int\":-42,\"nil\":null,\"unicode\":\"h\u00e9llo \u2265 \u65e5\u672c\u8a9e\"}",
            "blake3-512:37811291b195fd0360a628962fa4e71fe502419daf5dfdf37a0fcd1eb479280e"
                + "7df0d40b8ccb380d6bb6c2ec775f61b6d529da4b602303df124cd9538d6ae934"
        );
    }

    @Test
    void parserRoundTripsGeneratedCanonicalJson() {
        Jcs.Json value = Jcs.object(
            "xs", Jcs.array(List.of(Jcs.integer(1), Jcs.bool(true), Jcs.nullValue())),
            "name", Jcs.string("C.f(int)")
        );

        assertEquals(Jcs.encode(value), Jcs.encode(Jcs.parse(Jcs.encode(value))));
    }

    private static byte[] vectorInput(int inputLen) {
        byte[] out = new byte[inputLen];
        for (int i = 0; i < inputLen; i++) {
            out[i] = (byte) (i % 251);
        }
        return out;
    }

    private static void assertRustVector(Jcs.Json value, String expectedBytes, String expectedCid) {
        assertEquals(expectedBytes, Jcs.encode(value));
        assertEquals(expectedCid, Jcs.cid(value));
    }
}
