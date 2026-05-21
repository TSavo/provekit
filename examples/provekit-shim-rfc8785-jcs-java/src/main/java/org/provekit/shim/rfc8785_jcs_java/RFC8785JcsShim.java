// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-rfc8785-jcs-java: RFC 8785 (JSON Canonicalization Scheme) sugar shim.
//
// Self-contained implementation using Jackson for parsing + manual canonicalization
// per RFC 8785. Sister shim to provekit-shim-rfc8785-jcs-rust on
// concept:family:json-canonicalization. Concept names aligned 1:1.

package org.provekit.shim.rfc8785_jcs_java;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import com.provekit.lift.java_source.ProveKitSugar;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

/**
 * RFC 8785 JCS canonicalization in Java. Three public boundaries mirror
 * the rust sister's three @sugar entry points.
 */
public final class RFC8785JcsShim {

    private RFC8785JcsShim() {
        // Utility class.
    }

    private static final ObjectMapper MAPPER = new ObjectMapper();

    /**
     * {@code concept:rfc8785-jcs-encode} — canonicalize a JSON value tree.
     * Top-level entry point. Mirrors
     * {@code provekit-shim-rfc8785-jcs-rust::encode_jcs(v: &Value) -> String}.
     */
    @ProveKitSugar(
        concept = "concept:rfc8785-jcs-encode",
        library = "provekit-rfc8785-jcs-java",
        family = "concept:family:json-canonicalization",
        version = "0.1",
        loss = {}
    )
    public static String encode_jcs(JsonNode v) {
        StringBuilder out = new StringBuilder();
        encode_value(v, out);
        return out.toString();
    }

    /**
     * {@code concept:rfc8785-jcs-encode-value} — encode a JSON value into the
     * supplied buffer. Recursive helper. Mirrors
     * {@code provekit-shim-rfc8785-jcs-rust::encode_value(v: &Value, out: &mut String)}.
     */
    @ProveKitSugar(
        concept = "concept:rfc8785-jcs-encode-value",
        library = "provekit-rfc8785-jcs-java",
        family = "concept:family:json-canonicalization",
        version = "0.1",
        loss = {}
    )
    public static void encode_value(JsonNode v, StringBuilder out) {
        if (v == null || v.isNull()) {
            out.append("null");
        } else if (v.isBoolean()) {
            out.append(v.booleanValue() ? "true" : "false");
        } else if (v.isNumber()) {
            // RFC 8785 uses ECMA-262 number serialization. Java's
            // double-to-string is close but not identical; for now use the
            // textual form Jackson chose. (Full ECMA-262 conformance is a
            // separate loss dimension — declared in the rust sister's
            // future when minted.)
            out.append(v.asText());
        } else if (v.isTextual()) {
            encode_string(v.asText(), out);
        } else if (v.isArray()) {
            out.append('[');
            boolean first = true;
            for (JsonNode child : v) {
                if (!first) out.append(',');
                encode_value(child, out);
                first = false;
            }
            out.append(']');
        } else if (v.isObject()) {
            // RFC 8785 requires keys sorted by code-point order (after parsing
            // unescape).
            List<String> keys = new ArrayList<>();
            Iterator<String> it = v.fieldNames();
            while (it.hasNext()) keys.add(it.next());
            Collections.sort(keys);
            out.append('{');
            boolean first = true;
            for (String k : keys) {
                if (!first) out.append(',');
                encode_string(k, out);
                out.append(':');
                encode_value(v.get(k), out);
                first = false;
            }
            out.append('}');
        }
    }

    /**
     * {@code concept:rfc8785-jcs-encode-string} — encode a string with JSON
     * escaping per RFC 8785 §3.2.2.2 (escape control chars + backslash + quote).
     * Mirrors {@code provekit-shim-rfc8785-jcs-rust::encode_string(s: &str, out: &mut String)}.
     */
    @ProveKitSugar(
        concept = "concept:rfc8785-jcs-encode-string",
        library = "provekit-rfc8785-jcs-java",
        family = "concept:family:json-canonicalization",
        version = "0.1",
        loss = {}
    )
    public static void encode_string(String s, StringBuilder out) {
        out.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '"') {
                out.append("\\\"");
            } else if (c == '\\') {
                out.append("\\\\");
            } else if (c == '\b') {
                out.append("\\b");
            } else if (c == '\f') {
                out.append("\\f");
            } else if (c == '\n') {
                out.append("\\n");
            } else if (c == '\r') {
                out.append("\\r");
            } else if (c == '\t') {
                out.append("\\t");
            } else if (c < 0x20) {
                out.append(String.format("\\u%04x", (int) c));
            } else {
                out.append(c);
            }
        }
        out.append('"');
    }
}
