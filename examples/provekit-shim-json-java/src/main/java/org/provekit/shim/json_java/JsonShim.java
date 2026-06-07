// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-json-java: Jackson's @ProveKitSugar shim.
//
// Realizes concept:family:json concepts via Jackson (com.fasterxml.jackson.databind).
// Sister shim to:
//   - rust: provekit-shim-serde-json-rust (serde_json)
//   - python: provekit-shim-json-python (stdlib `json`, follow-up)
// All anchor to boundary:rfc8259-json.
//
// Substrate-honest concept naming: the 1:1 alignment with
// provekit-shim-serde-json-rust is the cross-library cluster signal.

package org.provekit.shim.json_java;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.core.JsonProcessingException;
import com.provekit.lift.java_source.ProveKitSugar;

/**
 * Java realizations of concept:family:json concepts via Jackson.
 * <p>
 * Concept names match provekit-shim-serde-json-rust 1:1 (concept:json-parse,
 * concept:json-serialize). The substrate recognizes cluster membership by
 * the concept name; the kit's catalog morphism maps between Jackson's
 * JsonNode and the substrate-canonical JSON value sort.
 */
public final class JsonShim {

    private JsonShim() {
        // Utility class.
    }

    /**
     * Shared mapper. Jackson convention is to reuse ObjectMapper instances —
     * they are thread-safe after configuration. Construction cost is amortized.
     */
    private static final ObjectMapper MAPPER = new ObjectMapper();

    /**
     * {@code concept:json-parse} — parse a JSON text into a JsonNode tree.
     * Mirrors {@code provekit-shim-serde-json-rust::json_parse(s: &str) -> Result<Value, String>}.
     * Java translates the Result error arm via a thrown RuntimeException
     * (the rust kit declares the morphism Result&lt;T,E&gt; → T with
     * exception-propagation loss).
     */
    @ProveKitSugar(
        concept = "concept:json-parse",
        library = "jackson",
        family = "concept:family:json",
        version = "2.17",
        loss = {}
    )
    public static JsonNode json_parse(String s) {
        try {
            return MAPPER.readTree(s);
        } catch (JsonProcessingException e) {
            throw new RuntimeException("json-parse: " + e.getMessage(), e);
        }
    }

    /**
     * {@code concept:json-serialize} — serialize a JsonNode tree to JSON text.
     * Mirrors {@code provekit-shim-serde-json-rust::json_serialize(v: &Value) -> Result<String, String>}.
     * <p>
     * Loss dimension {@code non-canonical-key-order} declared identically to
     * the rust sister: Jackson preserves insertion order, not canonical sort.
     * For canonical JSON (RFC 8785), use the rfc8785-jcs shim family instead.
     */
    @ProveKitSugar(
        concept = "concept:json-serialize",
        library = "jackson",
        family = "concept:family:json",
        version = "2.17",
        loss = {"non-canonical-key-order"}
    )
    public static String json_serialize(JsonNode v) {
        try {
            return MAPPER.writeValueAsString(v);
        } catch (JsonProcessingException e) {
            throw new RuntimeException("json-serialize: " + e.getMessage(), e);
        }
    }
}
