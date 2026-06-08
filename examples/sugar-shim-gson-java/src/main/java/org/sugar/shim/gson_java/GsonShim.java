// SPDX-License-Identifier: Apache-2.0
//
// sugar-shim-gson-java: Gson's @ProveKitSugar shim.
//
// SECOND java realization of concept:family:json. The first sister
// (sugar-shim-json-java) realizes via Jackson. Both members of the same
// family with identical concept names — proves the substrate's family-
// aware dispatch works WITHIN a language too: caller picks via --library.
//
// Substrate-honest concept naming: 1:1 alignment with
// sugar-shim-serde-json-rust + sugar-shim-json-java. The substrate
// recognizes cluster membership by the concept name; the kit's catalog
// morphism maps between Gson's JsonElement and the substrate-canonical
// JSON value sort.

package org.sugar.shim.gson_java;

import com.google.gson.Gson;
import com.google.gson.JsonElement;
import com.google.gson.JsonParser;
import com.google.gson.JsonSyntaxException;
import com.sugar.lift.java_source.ProveKitSugar;

/**
 * Java realizations of concept:family:json concepts via Gson.
 */
public final class GsonShim {

    private GsonShim() {
        // Utility class.
    }

    /** Shared Gson instance. Thread-safe for serialize/deserialize ops. */
    private static final Gson GSON = new Gson();

    /**
     * {@code concept:json-parse} — parse a JSON text into a JsonElement tree.
     * Mirrors {@code sugar-shim-serde-json-rust::json_parse(s: &str) -> Result<Value, String>}.
     */
    @ProveKitSugar(
        concept = "concept:json-parse",
        library = "gson",
        family = "concept:family:json",
        version = "2.11",
        loss = {}
    )
    public static JsonElement json_parse(String s) {
        try {
            return JsonParser.parseString(s);
        } catch (JsonSyntaxException e) {
            throw new RuntimeException("json-parse: " + e.getMessage(), e);
        }
    }

    /**
     * {@code concept:json-serialize} — serialize a JsonElement tree to JSON text.
     * Mirrors {@code sugar-shim-serde-json-rust::json_serialize(v: &Value) -> Result<String, String>}.
     *
     * Loss dimension {@code non-canonical-key-order} declared identically:
     * Gson preserves insertion order, not canonical sort.
     */
    @ProveKitSugar(
        concept = "concept:json-serialize",
        library = "gson",
        family = "concept:family:json",
        version = "2.11",
        loss = {"non-canonical-key-order"}
    )
    public static String json_serialize(JsonElement v) {
        return GSON.toJson(v);
    }
}
