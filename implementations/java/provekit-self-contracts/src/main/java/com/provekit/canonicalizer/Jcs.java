// SPDX-License-Identifier: Apache-2.0
//
// JCS-JSON encoder (RFC 8785). Java peer mirroring
// implementations/rust/provekit-canonicalizer/src/jcs.rs and
// implementations/csharp/Provekit.Canonicalizer/Jcs.cs 1:1.
//
// Rules (canonicalization grammar §7):
//   - Object keys sorted by Unicode code-point order. The protocol's
//     keys are all ASCII so byte-order suffices, but we sort by
//     codepoint for parity with the rust/csharp peers.
//   - Numbers: integers serialized as plain decimal digits (we only
//     carry int64; floats are not produced by the kit/mint flow).
//   - Strings: UTF-8 verbatim, escape `"` and `\` and U+0000..U+001F
//     as `\\u00XX` (lowercase hex). RFC 8785 also permits the named
//     short escapes (`\n` etc.); the C++/rust peers chose `\\u00XX`
//     for determinism — we match.
//   - true / false / null verbatim.
//   - No whitespace anywhere.
//
// The returned String's UTF-8 byte form IS the canonical wire form.

package com.provekit.canonicalizer;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

public final class Jcs {

    private Jcs() {}

    /**
     * Encode {@code v} as a JCS-JSON string per RFC 8785. The returned
     * string's UTF-8 byte form is the canonical wire form.
     */
    public static String encode(Value v) {
        StringBuilder sb = new StringBuilder();
        encodeValue(v, sb);
        return sb.toString();
    }

    /** Encode and return the raw UTF-8 bytes directly. */
    public static byte[] encodeUtf8(Value v) {
        return encode(v).getBytes(StandardCharsets.UTF_8);
    }

    private static void encodeValue(Value v, StringBuilder sb) {
        switch (v.kind()) {
            case NULL:
                sb.append("null");
                return;
            case BOOL:
                sb.append(v.asBool() ? "true" : "false");
                return;
            case INTEGER:
                sb.append(Long.toString(v.asInt()));
                return;
            case STRING:
                encodeString(v.asString(), sb);
                return;
            case ARRAY:
                encodeArray(v.asArray(), sb);
                return;
            case OBJECT:
                encodeObject(v.asObject(), sb);
                return;
        }
        throw new IllegalStateException("unreachable: kind=" + v.kind());
    }

    private static void encodeArray(List<Value> items, StringBuilder sb) {
        sb.append('[');
        for (int i = 0; i < items.size(); i++) {
            if (i > 0) sb.append(',');
            encodeValue(items.get(i), sb);
        }
        sb.append(']');
    }

    private static void encodeObject(List<Map.Entry<String, Value>> entries, StringBuilder sb) {
        // RFC 8785 §3.2.3: keys sorted by Unicode code-point order.
        // For ASCII keys this is identical to byte order; for non-ASCII
        // keys we must compare the UTF-16 codepoints (with surrogate
        // pairs handled correctly). Java's String.compareTo compares
        // UTF-16 code units, which differs from codepoint order for
        // characters above U+FFFF. The protocol's keys are ASCII; we
        // sort by codepoints anyway for full RFC 8785 conformance.
        ArrayList<Map.Entry<String, Value>> sorted = new ArrayList<>(entries);
        sorted.sort((a, b) -> compareCodepoints(a.getKey(), b.getKey()));

        sb.append('{');
        for (int i = 0; i < sorted.size(); i++) {
            if (i > 0) sb.append(',');
            encodeString(sorted.get(i).getKey(), sb);
            sb.append(':');
            encodeValue(sorted.get(i).getValue(), sb);
        }
        sb.append('}');
    }

    /**
     * Compare two strings by Unicode code-point order (RFC 8785 §3.2.3).
     * Walks both strings via {@link String#codePointAt(int)} so that
     * supplementary characters compare as a single codepoint, not as a
     * surrogate pair.
     */
    static int compareCodepoints(String a, String b) {
        int ai = 0;
        int bi = 0;
        while (ai < a.length() && bi < b.length()) {
            int ca = a.codePointAt(ai);
            int cb = b.codePointAt(bi);
            if (ca != cb) {
                return Integer.compare(ca, cb);
            }
            ai += Character.charCount(ca);
            bi += Character.charCount(cb);
        }
        return Integer.compare(a.length() - ai, b.length() - bi);
    }

    // RFC 8785 §3.2.4: encode a string with JSON-required escaping only:
    //   "  → \"
    //   \  → \\
    //   U+0000..U+001F → \\u00XX (lowercase hex)
    //   all other code points: verbatim. Non-ASCII characters are kept
    //   as-is; the output's UTF-8 bytes equal the input's UTF-8 bytes.
    //
    // We iterate over UTF-16 code units. Surrogate pairs encode the
    // same supplementary code point identically in both UTF-16 and
    // UTF-8 once Encoding.UTF_8 is applied — emitting them verbatim
    // is correct.
    private static void encodeString(String s, StringBuilder sb) {
        sb.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '"') {
                sb.append("\\\"");
            } else if (c == '\\') {
                sb.append("\\\\");
            } else if (c < 0x20) {
                sb.append("\\u00");
                sb.append(hexLower((c >> 4) & 0xF));
                sb.append(hexLower(c & 0xF));
            } else {
                sb.append(c);
            }
        }
        sb.append('"');
    }

    private static char hexLower(int n) {
        return (char) (n < 10 ? ('0' + n) : ('a' + (n - 10)));
    }
}
