// SPDX-License-Identifier: Apache-2.0
//
// Java kit literal-encoding answers.
//
// Implements the provekit.plugin.literal_encoding_answers RPC method.
// Returns one LiteralEncodingMemento per sort the Java kit admits at
// literal positions per its SortAdmission declaration.
//
// Java admits: Int, Float, String, Bool, Bytes, Null (full primitive tier).
//
// CID computation: JCS(memento WITHOUT cid + kit_cid) -> blake3-512.
// JCS field order for LiteralEncodingMemento (alphabetical):
//   expected_term_shape_node, kind, language, schemaVersion, sort_cid, source_example
// JCS field order for expected_term_shape_node (alphabetical):
//   concept_name, sort, value
//
// Float value: Java's Term.Value.Real(double) emits "String.valueOf(3.14)" = "3.14"
// as a JSON number. We build the JCS string manually for the float case because
// Jcs.Value only supports integer JSON numbers.

package com.provekit.realize;

import com.provekit.ir.Blake3;
import com.provekit.ir.Jcs;
import com.provekit.ir.Jcs.Value;

import java.nio.charset.StandardCharsets;

/** Builds and caches the literal_encoding_answers JSON response for the Java kit. */
public final class LiteralEncodingAnswers {
    private LiteralEncodingAnswers() {}

    // Canonical sort CIDs (from #1282)
    // Java admits all 6 primitive sorts: Int, Float, String, Bool, Bytes, Null
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

    private static final String CONCEPT_LITERAL_NAME = "concept:literal";
    private static final String SCHEMA_VERSION = "1.0.0";
    private static final String KIND = "literal-encoding-memento";
    private static final String LANGUAGE = "java";

    // Cached JSON response (built once on first call)
    private static volatile String cachedJson = null;

    /**
     * Returns the complete JSON string for the provekit.plugin.literal_encoding_answers result.
     * Thread-safe via double-checked locking.
     */
    public static String toJson() {
        if (cachedJson == null) {
            synchronized (LiteralEncodingAnswers.class) {
                if (cachedJson == null) {
                    cachedJson = buildJson();
                }
            }
        }
        return cachedJson;
    }

    private static String buildJson() {
        String kitCid = Blake3.blake3_512(
            "provekit-realize-java-core@0.1.0".getBytes(StandardCharsets.UTF_8));

        // Java admits: Int, Float, String, Bool, Bytes, Null
        String intMemento     = buildMemento(kitCid, SORT_INT_CID,    "42",       intValue(42));
        String floatMemento   = buildMementoRaw(kitCid, SORT_FLOAT_CID, "3.14",   "3.14");
        String stringMemento  = buildMemento(kitCid, SORT_STRING_CID, "\"hello\"", stringValue("hello"));
        String boolMemento    = buildMemento(kitCid, SORT_BOOL_CID,   "true",     boolValue(true));
        String bytesMemento   = buildMemento(kitCid, SORT_BYTES_CID,  "\"abc\".getBytes()", stringValue("abc"));
        String nullMemento    = buildMemento(kitCid, SORT_NULL_CID,   "null",     nullValue());

        return "{\"answers\":["
            + intMemento + ","
            + floatMemento + ","
            + stringMemento + ","
            + boolMemento + ","
            + bytesMemento + ","
            + nullMemento
            + "]}";
    }

    /**
     * Builds a LiteralEncodingMemento JSON string for sorts whose values
     * can be represented in Jcs.Value (all sorts except Float).
     *
     * @param valueJson the JCS-encoded value fragment (e.g., "42", "\"hello\"", "true", "null")
     */
    private static String buildMemento(String kitCid, String sortCid,
                                        String sourceExample, Value valueNode) {
        // CID computation: JCS(memento WITHOUT cid + kit_cid)
        // JCS field order for forCid (alphabetical):
        //   expected_term_shape_node, kind, language, schemaVersion, sort_cid, source_example
        // JCS field order for expected_term_shape_node (alphabetical):
        //   concept_name, sort, value
        Value expectedTermShapeNode = Value.object(
            "concept_name", Value.string(CONCEPT_LITERAL_NAME),
            "sort", Value.string(sortCid),
            "value", valueNode
        );
        Value forCid = Value.object(
            "expected_term_shape_node", expectedTermShapeNode,
            "kind", Value.string(KIND),
            "language", Value.string(LANGUAGE),
            "schemaVersion", Value.string(SCHEMA_VERSION),
            "sort_cid", Value.string(sortCid),
            "source_example", Value.string(sourceExample)
        );
        String cid = Jcs.blake3Cid(forCid);
        String etsnJson = Jcs.encode(expectedTermShapeNode);

        // Wire JSON (fields in arbitrary order; libprovekit deserializes by key)
        return "{\"cid\":" + JsonUtil.quoted(cid)
            + ",\"expected_term_shape_node\":" + etsnJson
            + ",\"kind\":" + JsonUtil.quoted(KIND)
            + ",\"kit_cid\":" + JsonUtil.quoted(kitCid)
            + ",\"language\":" + JsonUtil.quoted(LANGUAGE)
            + ",\"schemaVersion\":" + JsonUtil.quoted(SCHEMA_VERSION)
            + ",\"sort_cid\":" + JsonUtil.quoted(sortCid)
            + ",\"source_example\":" + JsonUtil.quoted(sourceExample)
            + "}";
    }

    /**
     * Builds a LiteralEncodingMemento JSON string for Float sort by constructing
     * the JCS CID-input string manually (Jcs.Value doesn't support float numbers).
     *
     * The JCS for the forCid object is built manually with sorted keys and the
     * float value emitted as a plain JSON number (no quotes), matching what
     * Java's Term.Value.Real(double).toJson() produces.
     */
    private static String buildMementoRaw(String kitCid, String sortCid,
                                           String sourceExample, String valueJsonLiteral) {
        // Manually build the JCS string for forCid (keys alphabetically sorted)
        // expected_term_shape_node: {concept_name, sort, value}
        // forCid: {expected_term_shape_node, kind, language, schemaVersion, sort_cid, source_example}
        String etsnForCid = "{\"concept_name\":" + JsonUtil.quoted(CONCEPT_LITERAL_NAME)
            + ",\"sort\":" + JsonUtil.quoted(sortCid)
            + ",\"value\":" + valueJsonLiteral
            + "}";
        String forCidJcs = "{\"expected_term_shape_node\":" + etsnForCid
            + ",\"kind\":" + JsonUtil.quoted(KIND)
            + ",\"language\":" + JsonUtil.quoted(LANGUAGE)
            + ",\"schemaVersion\":" + JsonUtil.quoted(SCHEMA_VERSION)
            + ",\"sort_cid\":" + JsonUtil.quoted(sortCid)
            + ",\"source_example\":" + JsonUtil.quoted(sourceExample)
            + "}";
        String cid = Blake3.blake3_512(forCidJcs.getBytes(StandardCharsets.UTF_8));

        // The expected_term_shape_node in the wire response also contains the float value
        String etsnJson = "{\"concept_name\":" + JsonUtil.quoted(CONCEPT_LITERAL_NAME)
            + ",\"sort\":" + JsonUtil.quoted(sortCid)
            + ",\"value\":" + valueJsonLiteral
            + "}";
        return "{\"cid\":" + JsonUtil.quoted(cid)
            + ",\"expected_term_shape_node\":" + etsnJson
            + ",\"kind\":" + JsonUtil.quoted(KIND)
            + ",\"kit_cid\":" + JsonUtil.quoted(kitCid)
            + ",\"language\":" + JsonUtil.quoted(LANGUAGE)
            + ",\"schemaVersion\":" + JsonUtil.quoted(SCHEMA_VERSION)
            + ",\"sort_cid\":" + JsonUtil.quoted(sortCid)
            + ",\"source_example\":" + JsonUtil.quoted(sourceExample)
            + "}";
    }

    // --- Value helpers ---

    private static Value intValue(long n) {
        return Value.integer(n);
    }

    private static Value stringValue(String s) {
        return Value.string(s);
    }

    private static Value boolValue(boolean b) {
        return Value.bool(b);
    }

    private static Value nullValue() {
        return Value.NULL;
    }
}
