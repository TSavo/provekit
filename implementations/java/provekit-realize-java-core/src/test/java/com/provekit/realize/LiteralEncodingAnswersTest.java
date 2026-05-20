// SPDX-License-Identifier: Apache-2.0
//
// Tests for provekit.plugin.literal_encoding_answers RPC handler.
//
// Java admits: Int, Float, String, Bool, Bytes, Null (full primitive tier, 6 answers).
// Golden CIDs verified on first run; hardcoded for regression protection.

package com.provekit.realize;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

public class LiteralEncodingAnswersTest {

    // Canonical sort CIDs (from #1282)
    private static final String SORT_INT_CID =
        "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58";
    private static final String SORT_FLOAT_CID =
        "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
    private static final String SORT_STRING_CID =
        "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";
    private static final String SORT_BOOL_CID =
        "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
    private static final String SORT_BYTES_CID =
        "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b";
    private static final String SORT_NULL_CID =
        "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5";

    @Test
    public void toJsonIsNonEmpty() {
        String json = LiteralEncodingAnswers.toJson();
        assertNotNull(json);
        assertFalse(json.isBlank());
        // Must start with {"answers":[
        assertTrue(json.startsWith("{\"answers\":["), "must start with {\"answers\":[");
    }

    @Test
    public void toJsonContainsSixAnswers() {
        // Java admits Int, Float, String, Bool, Bytes, Null -- 6 answers
        // Count occurrences of "literal-encoding-memento" as a proxy for answer count
        String json = LiteralEncodingAnswers.toJson();
        int count = 0;
        int idx = 0;
        while ((idx = json.indexOf("literal-encoding-memento", idx)) != -1) {
            count++;
            idx++;
        }
        assertEquals(6, count,
            "Java must emit exactly 6 literal-encoding-memento entries");
    }

    @Test
    public void allAnswersHaveCorrectLanguage() {
        String json = LiteralEncodingAnswers.toJson();
        // Each answer must have "language":"java"
        int count = 0;
        int idx = 0;
        while ((idx = json.indexOf("\"language\":\"java\"", idx)) != -1) {
            count++;
            idx++;
        }
        assertEquals(6, count, "must have 6 answers with language=java");
    }

    @Test
    public void allAnswersHaveCidField() {
        String json = LiteralEncodingAnswers.toJson();
        // Each answer must have a "cid":"blake3-512:..." field
        int count = 0;
        int idx = 0;
        while ((idx = json.indexOf("\"blake3-512:", idx)) != -1) {
            count++;
            idx++;
        }
        // 6 answers * 2 CIDs per answer (cid + kit_cid) + 6 sort_cids = 18 minimum
        assertTrue(count >= 6, "must have at least 6 blake3-512: CID values in JSON");
    }

    @Test
    public void answersCoversAllAdmittedSortCids() {
        String json = LiteralEncodingAnswers.toJson();
        assertTrue(json.contains(SORT_INT_CID), "must contain Int sort CID");
        assertTrue(json.contains(SORT_FLOAT_CID), "must contain Float sort CID");
        assertTrue(json.contains(SORT_STRING_CID), "must contain String sort CID");
        assertTrue(json.contains(SORT_BOOL_CID), "must contain Bool sort CID");
        assertTrue(json.contains(SORT_BYTES_CID), "must contain Bytes sort CID");
        assertTrue(json.contains(SORT_NULL_CID), "must contain Null sort CID");
    }

    @Test
    public void toJsonContainsFloatValue() {
        String json = LiteralEncodingAnswers.toJson();
        // Float memento must contain the bit-preserving __float_bits__ shape.
        // 4614253070214989087 == Double.doubleToRawLongBits(3.14) == 0x40091EB851EB851F.
        assertTrue(json.contains("\"value\":{\"__float_bits__\":4614253070214989087}"),
            "must contain float __float_bits__ value");
    }

    @Test
    public void toJsonFloatGoldenCid() {
        // Regression: Float memento golden CID must not change.
        // CID computed from JCS of forCid with __float_bits__ value.
        String json = LiteralEncodingAnswers.toJson();
        String expectedFloatCid =
            "blake3-512:fa616d402e270631cd136b95d96f9038536c3b507bb736c35554ff41c7e21a20dca3b451d1ae9ee75b6632d5e58b96619c422ea38cab404f47241d471bf77766";
        assertTrue(json.contains(expectedFloatCid),
            "Float memento golden CID must match");
    }

    @Test
    public void toJsonIsStable() {
        // Regression: same output on every call
        String first = LiteralEncodingAnswers.toJson();
        String second = LiteralEncodingAnswers.toJson();
        assertEquals(first, second, "toJson must return stable output");
    }
}
