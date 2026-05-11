// SPDX-License-Identifier: Apache-2.0
//
// JCS-JSON encoder (RFC 8785 / "JSON Canonicalization Scheme") plus
// BLAKE3-512 CIDs for Java IR and envelope tooling.
//
// Rules (RFC 8785 + protocol/specs/2026-04-30-canonicalization-grammar.md
// pass 7):
//   - Object keys sorted by Unicode code-point order.
//   - Numbers: integers serialized as plain decimal digits (the Java kit
//     only produces integer JSON numbers for canonical substrate values).
//   - Strings: UTF-8 verbatim; escape double-quote and backslash and
//     U+0000..U+001F as backslash-u-00XX (lowercase hex).
//   - true / false / null verbatim.
//   - No whitespace anywhere.
//
// Mirrors implementations/rust/provekit-canonicalizer/src/jcs.rs and
// implementations/csharp/Provekit.Canonicalizer/Jcs.cs 1:1.

package com.provekit.ir;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Objects;

/** Canonical JSON value trees plus BLAKE3-512 CIDs for ProvekIt Java tools. */
public final class Jcs {
    private Jcs() {}

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

    public sealed interface Json permits Null, Bool, Num, Str, Arr, Obj {}

    public record Null() implements Json {}
    public record Bool(boolean value) implements Json {}
    public record Num(long value) implements Json {}
    public record Str(String value) implements Json {}

    public record Arr(List<Json> values) implements Json {
        public Arr {
            values = List.copyOf(values);
        }

        public Json get(int index) {
            return values.get(index);
        }

        public Obj objectAt(int index) {
            return (Obj) values.get(index);
        }

        public Str stringAt(int index) {
            return (Str) values.get(index);
        }

        public boolean isEmpty() {
            return values.isEmpty();
        }
    }

    public record Field(String key, Json value) {}

    public record Obj(List<Field> fields) implements Json {
        public Obj {
            fields = List.copyOf(fields);
        }

        public Json get(String key) {
            for (Field field : fields) {
                if (field.key().equals(key)) return field.value();
            }
            return null;
        }

        public String stringField(String key) {
            Json value = get(key);
            if (value instanceof Str s) return s.value();
            throw new IllegalArgumentException("field is not a string: " + key);
        }

        public String stringFieldOrNull(String key) {
            Json value = get(key);
            return value instanceof Str s ? s.value() : null;
        }

        public boolean boolField(String key) {
            Json value = get(key);
            if (value instanceof Bool b) return b.value();
            throw new IllegalArgumentException("field is not a boolean: " + key);
        }

        public Obj objectField(String key) {
            Json value = get(key);
            if (value instanceof Obj o) return o;
            throw new IllegalArgumentException("field is not an object: " + key);
        }

        public Arr arrayField(String key) {
            Json value = get(key);
            if (value instanceof Arr a) return a;
            throw new IllegalArgumentException("field is not an array: " + key);
        }
    }

    public static Json nullValue() {
        return new Null();
    }

    public static Json bool(boolean value) {
        return new Bool(value);
    }

    public static Json integer(long value) {
        return new Num(value);
    }

    public static Str string(String value) {
        return new Str(value);
    }

    public static Arr array(Json... values) {
        return new Arr(List.of(values));
    }

    public static Arr array(List<? extends Json> values) {
        return new Arr(new ArrayList<>(values));
    }

    public static Obj object(Object... keyValues) {
        if (keyValues.length % 2 != 0) {
            throw new IllegalArgumentException("object requires key/value pairs");
        }
        List<Field> fields = new ArrayList<>();
        for (int i = 0; i < keyValues.length; i += 2) {
            Object key = keyValues[i];
            Object value = keyValues[i + 1];
            if (!(key instanceof String s)) {
                throw new IllegalArgumentException("object key must be a string");
            }
            if (!(value instanceof Json json)) {
                throw new IllegalArgumentException("object value for " + s + " is not Jcs.Json");
            }
            fields.add(new Field(s, json));
        }
        return new Obj(fields);
    }

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

    /** Encode {@code value} to a JCS-canonical UTF-8 string. */
    public static String encode(Json value) {
        return encode(toValue(value));
    }

    /** Encode {@code value} to JCS-canonical UTF-8 bytes. */
    public static byte[] encodeUtf8(Json value) {
        return encode(value).getBytes(StandardCharsets.UTF_8);
    }

    /** Convenience: BLAKE3-512 of JCS({@code v}). */
    public static String blake3Cid(Value v) {
        return Blake3.blake3_512(encodeUtf8(v));
    }

    /** Compatibility alias for callers that hash IR {@link Json} values. */
    public static String cid(Json value) {
        return Blake3.blake3_512(encodeUtf8(value));
    }

    /** Compatibility alias for raw BLAKE3-512 CIDs. */
    public static String blake3_512(byte[] input) {
        return Blake3.blake3_512(input);
    }

    public static Json parse(String json) {
        Parser parser = new Parser(json);
        Json value = parser.parseValue();
        parser.skipWhitespace();
        if (!parser.isEof()) throw parser.error("trailing content");
        return value;
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

    private static Value toValue(Json value) {
        if (value instanceof Null) {
            return Value.NULL;
        } else if (value instanceof Bool b) {
            return Value.bool(b.value());
        } else if (value instanceof Num n) {
            return Value.integer(n.value());
        } else if (value instanceof Str s) {
            return Value.string(s.value());
        } else if (value instanceof Arr a) {
            return Value.array(a.values().stream().map(Jcs::toValue).toList());
        } else if (value instanceof Obj o) {
            LinkedHashMap<String, Value> entries = new LinkedHashMap<>();
            for (Field field : o.fields()) {
                entries.put(field.key(), toValue(field.value()));
            }
            return Value.object(entries);
        }
        throw new IllegalArgumentException("unknown JSON value");
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

    private static final class Parser {
        private final String input;
        private int pos;

        Parser(String input) {
            this.input = input;
        }

        boolean isEof() {
            return pos >= input.length();
        }

        void skipWhitespace() {
            while (!isEof()) {
                char c = input.charAt(pos);
                if (c == ' ' || c == '\n' || c == '\r' || c == '\t') pos++;
                else break;
            }
        }

        Json parseValue() {
            skipWhitespace();
            if (isEof()) throw error("expected JSON value");
            char c = input.charAt(pos);
            return switch (c) {
                case '{' -> parseObject();
                case '[' -> parseArray();
                case '"' -> string(parseString());
                case 't' -> parseLiteral("true", bool(true));
                case 'f' -> parseLiteral("false", bool(false));
                case 'n' -> parseLiteral("null", nullValue());
                default -> {
                    if (c == '-' || (c >= '0' && c <= '9')) yield parseNumber();
                    throw error("unexpected JSON value");
                }
            };
        }

        private Json parseLiteral(String literal, Json value) {
            if (!input.startsWith(literal, pos)) throw error("expected " + literal);
            pos += literal.length();
            return value;
        }

        private Json parseNumber() {
            int start = pos;
            if (input.charAt(pos) == '-') pos++;
            while (!isEof() && Character.isDigit(input.charAt(pos))) pos++;
            if (!isEof() && (input.charAt(pos) == '.' || input.charAt(pos) == 'e' || input.charAt(pos) == 'E')) {
                throw error("JCS helper only accepts integer JSON numbers");
            }
            return integer(Long.parseLong(input.substring(start, pos)));
        }

        private Obj parseObject() {
            expect('{');
            List<Field> fields = new ArrayList<>();
            skipWhitespace();
            if (peek('}')) {
                pos++;
                return new Obj(fields);
            }
            while (true) {
                skipWhitespace();
                String key = parseString();
                skipWhitespace();
                expect(':');
                Json value = parseValue();
                fields.add(new Field(key, value));
                skipWhitespace();
                if (peek('}')) {
                    pos++;
                    break;
                }
                expect(',');
            }
            return new Obj(fields);
        }

        private Arr parseArray() {
            expect('[');
            List<Json> values = new ArrayList<>();
            skipWhitespace();
            if (peek(']')) {
                pos++;
                return new Arr(values);
            }
            while (true) {
                values.add(parseValue());
                skipWhitespace();
                if (peek(']')) {
                    pos++;
                    break;
                }
                expect(',');
            }
            return new Arr(values);
        }

        private String parseString() {
            expect('"');
            StringBuilder out = new StringBuilder();
            while (!isEof()) {
                char c = input.charAt(pos++);
                if (c == '"') return out.toString();
                if (c == '\\') {
                    if (isEof()) throw error("unterminated escape");
                    char e = input.charAt(pos++);
                    switch (e) {
                        case '"' -> out.append('"');
                        case '\\' -> out.append('\\');
                        case '/' -> out.append('/');
                        case 'b' -> out.append('\b');
                        case 'f' -> out.append('\f');
                        case 'n' -> out.append('\n');
                        case 'r' -> out.append('\r');
                        case 't' -> out.append('\t');
                        case 'u' -> {
                            if (pos + 4 > input.length()) throw error("short unicode escape");
                            int cp = java.lang.Integer.parseInt(input.substring(pos, pos + 4), 16);
                            out.append((char) cp);
                            pos += 4;
                        }
                        default -> throw error("bad escape");
                    }
                } else {
                    out.append(c);
                }
            }
            throw error("unterminated string");
        }

        private boolean peek(char c) {
            return !isEof() && input.charAt(pos) == c;
        }

        private void expect(char c) {
            if (isEof() || input.charAt(pos) != c) throw error("expected '" + c + "'");
            pos++;
        }

        IllegalArgumentException error(String message) {
            return new IllegalArgumentException(message + " at byte " + pos);
        }
    }
}
