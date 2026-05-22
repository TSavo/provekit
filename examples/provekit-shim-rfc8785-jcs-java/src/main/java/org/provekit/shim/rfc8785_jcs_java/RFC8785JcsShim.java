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
        // Inherits the loss from its recursive call into encode_value
        // (number canonicalization is per-value, not per-top-level call).
        loss = {"rfc8785-number-serialization-non-ecma262"}
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
        // Substrate-honest loss declaration: this realization uses
        // JsonNode.asText() for numbers, which Jackson chooses textually
        // and is NOT ECMA-262 §7.1.12.1 compliant (RFC 8785 §3.2.2.3).
        // Recoverable by routing numbers through a proper ECMA-262
        // formatter (concept:ecma262-number-format, to be minted).
        loss = {"rfc8785-number-serialization-non-ecma262"}
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
            // RFC 8785 §3.2.3 requires keys sorted by Unicode CODE-POINT
            // order. java's String.compareTo (used by Collections.sort)
            // uses UTF-16 CODE-UNIT order which disagrees for supplementary
            // characters: surrogate pairs (0xD800-0xDFFF code units) sort
            // BEFORE 0xE000-0xFFFF in UTF-16 lex order, but their decoded
            // code points are 0x10000+ — AFTER 0xE000-0xFFFF in code-point
            // order. Sort by iterating code points inline (materialized
            // body cannot reference static helpers on this class).
            List<String> keys = new ArrayList<>();
            Iterator<String> it = v.fieldNames();
            while (it.hasNext()) keys.add(it.next());
            keys.sort((a, b) -> {
                int i = 0, j = 0;
                int alen = a.length(), blen = b.length();
                while (i < alen && j < blen) {
                    int cpA = a.codePointAt(i);
                    int cpB = b.codePointAt(j);
                    if (cpA != cpB) return Integer.compare(cpA, cpB);
                    i += Character.charCount(cpA);
                    j += Character.charCount(cpB);
                }
                return Integer.compare(alen - i, blen - j);
            });
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
