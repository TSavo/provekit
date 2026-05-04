// SPDX-License-Identifier: Apache-2.0
//
// JCS-JSON encoder (RFC 8785 / "JSON Canonicalization Scheme") plus a
// minimal {@link Value} algebraic type used as input.
//
// Rules (RFC 8785 + protocol/specs/2026-04-30-canonicalization-grammar.md
// pass 7):
//   - Object keys sorted by Unicode code-point order. For ASCII-only
//     keys this collapses to byte-order; the protocol's keys are all
//     ASCII so byte-order suffices and matches the rust/cpp/csharp peers.
//   - Numbers: integers serialized as plain decimal digits (the kit
//     never produces floats from the mint flow).
//   - Strings: UTF-8 verbatim; escape double-quote and backslash and
//     U+0000..U+001F as backslash-u-00XX (lowercase hex). RFC 8785
//     also permits the named short escapes but the rust/cpp/csharp
//     peers chose the u-form for determinism; we match.
//   - true / false / null verbatim.
//   - No whitespace anywhere.
//
// Mirrors implementations/rust/provekit-canonicalizer/src/jcs.rs and
// implementations/csharp/Provekit.Canonicalizer/Jcs.cs 1:1.

package com.provekit.claimenvelope;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

public final class Jcs {

    private Jcs() {}

    // -------------------------------------------------------------------
    // Value type
    // -------------------------------------------------------------------

    /**
     * A simple algebraic value tree mirroring the rust/csharp peers'
     * {@code Value}. Objects preserve insertion order at construction
     * time, but the JCS encoder always emits keys in code-point order.
     */
    public static abstract sealed class Value
            permits Value.Null, Value.Bool, Value.Integer,
                    Value.Str, Value.Arr, Value.Obj {

        public static final Value NULL = new Null();

        public static Value bool(boolean b) {
            return b ? Bool.TRUE : Bool.FALSE;
        }

        public static Value integer(long n) {
            return new Integer(n);
        }

        public static Value string(String s) {
            return new Str(Objects.requireNonNull(s, "string value"));
        }

        public static Value array(List<Value> items) {
            return new Arr(List.copyOf(items));
        }

        public static Value array(Value... items) {
            return new Arr(List.of(items));
        }

        /** Build an object from an even-length list of (key, value, key, value, ...) pairs. */
        public static Value object(Object... kvs) {
            if ((kvs.length & 1) != 0) {
                throw new IllegalArgumentException("object: requires an even number of arguments");
            }
            LinkedHashMap<String, Value> entries = new LinkedHashMap<>();
            for (int i = 0; i < kvs.length; i += 2) {
                if (!(kvs[i] instanceof String key)) {
                    throw new IllegalArgumentException("object: keys must be String");
                }
                if (!(kvs[i + 1] instanceof Value v)) {
                    throw new IllegalArgumentException("object: values must be Value");
                }
                entries.put(key, v);
            }
            return new Obj(entries);
        }

        public static Value object(LinkedHashMap<String, Value> entries) {
            return new Obj(new LinkedHashMap<>(entries));
        }

        public static final class Null extends Value {
            private Null() {}
        }

        public static final class Bool extends Value {
            public static final Bool TRUE = new Bool(true);
            public static final Bool FALSE = new Bool(false);
            public final boolean value;
            private Bool(boolean v) { this.value = v; }
        }

        public static final class Integer extends Value {
            public final long value;
            private Integer(long v) { this.value = v; }
        }

        public static final class Str extends Value {
            public final String value;
            private Str(String v) { this.value = v; }
        }

        public static final class Arr extends Value {
            public final List<Value> items;
            private Arr(List<Value> items) { this.items = items; }
        }

        public static final class Obj extends Value {
            public final LinkedHashMap<String, Value> entries;
            private Obj(LinkedHashMap<String, Value> entries) { this.entries = entries; }
        }
    }

    // -------------------------------------------------------------------
    // Encoder
    // -------------------------------------------------------------------

    /** Encode {@code v} to a JCS-canonical UTF-8 string. */
    public static String encode(Value v) {
        StringBuilder sb = new StringBuilder();
        encodeValue(v, sb);
        return sb.toString();
    }

    /** Encode {@code v} to JCS-canonical UTF-8 bytes. */
    public static byte[] encodeUtf8(Value v) {
        return encode(v).getBytes(StandardCharsets.UTF_8);
    }

    private static void encodeValue(Value v, StringBuilder out) {
        if (v instanceof Value.Null) {
            out.append("null");
        } else if (v instanceof Value.Bool b) {
            out.append(b.value ? "true" : "false");
        } else if (v instanceof Value.Integer i) {
            out.append(java.lang.Long.toString(i.value));
        } else if (v instanceof Value.Str s) {
            encodeString(s.value, out);
        } else if (v instanceof Value.Arr a) {
            out.append('[');
            boolean first = true;
            for (Value item : a.items) {
                if (!first) out.append(',');
                first = false;
                encodeValue(item, out);
            }
            out.append(']');
        } else if (v instanceof Value.Obj o) {
            // Sort keys by Unicode code-point order. String#compareTo() in
            // Java compares char-by-char which is UTF-16 code-unit order;
            // that diverges from code-point order ONLY for chars in the
            // surrogate range, which is irrelevant for the protocol's
            // ASCII-only keys. The rust peer does byte-cmp; for ASCII keys
            // both orderings collapse to byte order.
            List<String> keys = new ArrayList<>(o.entries.keySet());
            Collections.sort(keys, Jcs::compareCodePoints);
            out.append('{');
            boolean first = true;
            for (String k : keys) {
                if (!first) out.append(',');
                first = false;
                encodeString(k, out);
                out.append(':');
                encodeValue(o.entries.get(k), out);
            }
            out.append('}');
        } else {
            throw new IllegalStateException("Unknown Value variant: " + v.getClass());
        }
    }

    private static int compareCodePoints(String a, String b) {
        int i = 0, j = 0;
        while (i < a.length() && j < b.length()) {
            int ca = a.codePointAt(i);
            int cb = b.codePointAt(j);
            if (ca != cb) return java.lang.Integer.compare(ca, cb);
            i += Character.charCount(ca);
            j += Character.charCount(cb);
        }
        return java.lang.Integer.compare(a.length() - i, b.length() - j);
    }

    private static void encodeString(String s, StringBuilder out) {
        out.append('"');
        // Iterate by Unicode code points so non-ASCII chars round-trip
        // verbatim; pushing a code point into a StringBuilder re-encodes
        // it as the same UTF-8 bytes when the result is later UTF-8
        // serialized. This matches the rust peer's char iteration.
        int i = 0;
        while (i < s.length()) {
            int cp = s.codePointAt(i);
            i += Character.charCount(cp);
            if (cp == '"') {
                out.append("\\\"");
            } else if (cp == '\\') {
                out.append("\\\\");
            } else if (cp < 0x20) {
                out.append("\\u00");
                out.append(HEX[(cp >>> 4) & 0xF]);
                out.append(HEX[cp & 0xF]);
            } else {
                out.appendCodePoint(cp);
            }
        }
        out.append('"');
    }

    private static final char[] HEX = {
        '0','1','2','3','4','5','6','7','8','9','a','b','c','d','e','f'
    };

    /** Convenience: BLAKE3-512 of JCS({@code v}). */
    public static String blake3Cid(Value v) {
        return Blake3.blake3_512(encodeUtf8(v));
    }
}
