package com.provekit.ir;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/** Canonical JSON value tree plus BLAKE3-512 CIDs for ProvekIt Java tools. */
public final class Jcs {
    private Jcs() {}

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

    public static String encode(Json value) {
        StringBuilder out = new StringBuilder();
        encodeValue(value, out);
        return out.toString();
    }

    public static String cid(Json value) {
        return blake3_512(encode(value).getBytes(StandardCharsets.UTF_8));
    }

    public static String blake3_512(byte[] input) {
        return "blake3-512:" + Blake3.hashHex(input, 64);
    }

    public static Json parse(String json) {
        Parser parser = new Parser(json);
        Json value = parser.parseValue();
        parser.skipWhitespace();
        if (!parser.isEof()) throw parser.error("trailing content");
        return value;
    }

    private static void encodeValue(Json value, StringBuilder out) {
        if (value instanceof Null) {
            out.append("null");
        } else if (value instanceof Bool b) {
            out.append(b.value() ? "true" : "false");
        } else if (value instanceof Num n) {
            out.append(n.value());
        } else if (value instanceof Str s) {
            encodeString(s.value(), out);
        } else if (value instanceof Arr a) {
            out.append('[');
            for (int i = 0; i < a.values().size(); i++) {
                if (i > 0) out.append(',');
                encodeValue(a.values().get(i), out);
            }
            out.append(']');
        } else if (value instanceof Obj o) {
            List<Field> sorted = new ArrayList<>(o.fields());
            sorted.sort(Comparator.comparing(Field::key));
            out.append('{');
            for (int i = 0; i < sorted.size(); i++) {
                if (i > 0) out.append(',');
                Field field = sorted.get(i);
                encodeString(field.key(), out);
                out.append(':');
                encodeValue(field.value(), out);
            }
            out.append('}');
        } else {
            throw new IllegalArgumentException("unknown JSON value");
        }
    }

    private static void encodeString(String value, StringBuilder out) {
        out.append('"');
        for (int i = 0; i < value.length();) {
            int cp = value.codePointAt(i);
            i += Character.charCount(cp);
            if (cp == '"') {
                out.append("\\\"");
            } else if (cp == '\\') {
                out.append("\\\\");
            } else if (cp < 0x20) {
                out.append("\\u00");
                out.append(Character.forDigit((cp >> 4) & 0xf, 16));
                out.append(Character.forDigit(cp & 0xf, 16));
            } else {
                out.appendCodePoint(cp);
            }
        }
        out.append('"');
    }

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
                            int cp = Integer.parseInt(input.substring(pos, pos + 4), 16);
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

    private static final class Blake3 {
        private static final int BLOCK_LEN = 64;
        private static final int CHUNK_LEN = 1024;
        private static final int CHUNK_START = 1;
        private static final int CHUNK_END = 2;
        private static final int PARENT = 4;
        private static final int ROOT = 8;
        private static final int[] IV = {
            0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
            0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19
        };
        private static final int[] MSG_PERMUTATION = {2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8};

        static String hashHex(byte[] input, int outLen) {
            byte[] hash = hash(input, outLen);
            StringBuilder out = new StringBuilder(hash.length * 2);
            for (byte b : hash) out.append(String.format("%02x", b & 0xff));
            return out.toString();
        }

        private static byte[] hash(byte[] input, int outLen) {
            List<Output> level = new ArrayList<>();
            if (input.length == 0) {
                level.add(chunkOutput(input, 0, 0, 0));
            } else {
                int chunks = (input.length + CHUNK_LEN - 1) / CHUNK_LEN;
                for (int i = 0; i < chunks; i++) {
                    int offset = i * CHUNK_LEN;
                    int len = Math.min(CHUNK_LEN, input.length - offset);
                    level.add(chunkOutput(input, offset, len, i));
                }
            }
            while (level.size() > 1) {
                List<Output> next = new ArrayList<>();
                for (int i = 0; i < level.size(); i += 2) {
                    if (i + 1 == level.size()) {
                        next.add(level.get(i));
                    } else {
                        next.add(parentOutput(level.get(i).chainingValue(), level.get(i + 1).chainingValue()));
                    }
                }
                level = next;
            }
            return level.get(0).rootBytes(outLen);
        }

        private static Output chunkOutput(byte[] input, int offset, int len, long chunkCounter) {
            int[] cv = IV.clone();
            int blockOffset = 0;
            int blocksCompressed = 0;
            while (len - blockOffset > BLOCK_LEN) {
                int[] words = wordsFromBlock(input, offset + blockOffset, BLOCK_LEN);
                int flags = blocksCompressed == 0 ? CHUNK_START : 0;
                cv = first8(compress(cv, words, BLOCK_LEN, chunkCounter, flags));
                blocksCompressed++;
                blockOffset += BLOCK_LEN;
            }
            int remaining = len - blockOffset;
            int flags = CHUNK_END | (blocksCompressed == 0 ? CHUNK_START : 0);
            int[] words = wordsFromBlock(input, offset + blockOffset, remaining);
            return new Output(cv, words, remaining, chunkCounter, flags);
        }

        private static Output parentOutput(int[] left, int[] right) {
            int[] blockWords = new int[16];
            System.arraycopy(left, 0, blockWords, 0, 8);
            System.arraycopy(right, 0, blockWords, 8, 8);
            return new Output(IV.clone(), blockWords, BLOCK_LEN, 0, PARENT);
        }

        private record Output(int[] inputCv, int[] blockWords, int blockLen, long counter, int flags) {
            int[] chainingValue() {
                return first8(compress(inputCv, blockWords, blockLen, counter, flags));
            }

            byte[] rootBytes(int outLen) {
                byte[] out = new byte[outLen];
                int written = 0;
                long outputCounter = 0;
                while (written < outLen) {
                    int[] words = compress(inputCv, blockWords, blockLen, outputCounter, flags | ROOT);
                    byte[] block = wordsToBytes(words);
                    int take = Math.min(block.length, outLen - written);
                    System.arraycopy(block, 0, out, written, take);
                    written += take;
                    outputCounter++;
                }
                return out;
            }
        }

        private static int[] compress(int[] cv, int[] blockWords, int blockLen, long counter, int flags) {
            int[] state = new int[16];
            System.arraycopy(cv, 0, state, 0, 8);
            System.arraycopy(IV, 0, state, 8, 4);
            state[12] = (int) counter;
            state[13] = (int) (counter >>> 32);
            state[14] = blockLen;
            state[15] = flags;
            int[] m = blockWords.clone();
            for (int round = 0; round < 7; round++) {
                round(state, m);
                if (round != 6) m = permute(m);
            }
            for (int i = 0; i < 8; i++) {
                state[i] ^= state[i + 8];
                state[i + 8] ^= cv[i];
            }
            return state;
        }

        private static void round(int[] s, int[] m) {
            g(s, 0, 4, 8, 12, m[0], m[1]);
            g(s, 1, 5, 9, 13, m[2], m[3]);
            g(s, 2, 6, 10, 14, m[4], m[5]);
            g(s, 3, 7, 11, 15, m[6], m[7]);
            g(s, 0, 5, 10, 15, m[8], m[9]);
            g(s, 1, 6, 11, 12, m[10], m[11]);
            g(s, 2, 7, 8, 13, m[12], m[13]);
            g(s, 3, 4, 9, 14, m[14], m[15]);
        }

        private static void g(int[] s, int a, int b, int c, int d, int mx, int my) {
            s[a] = s[a] + s[b] + mx;
            s[d] = Integer.rotateRight(s[d] ^ s[a], 16);
            s[c] = s[c] + s[d];
            s[b] = Integer.rotateRight(s[b] ^ s[c], 12);
            s[a] = s[a] + s[b] + my;
            s[d] = Integer.rotateRight(s[d] ^ s[a], 8);
            s[c] = s[c] + s[d];
            s[b] = Integer.rotateRight(s[b] ^ s[c], 7);
        }

        private static int[] permute(int[] m) {
            int[] out = new int[16];
            for (int i = 0; i < 16; i++) out[i] = m[MSG_PERMUTATION[i]];
            return out;
        }

        private static int[] wordsFromBlock(byte[] input, int offset, int len) {
            byte[] block = new byte[BLOCK_LEN];
            if (len > 0) System.arraycopy(input, offset, block, 0, len);
            int[] words = new int[16];
            for (int i = 0; i < 16; i++) {
                int j = i * 4;
                words[i] = (block[j] & 0xff)
                    | ((block[j + 1] & 0xff) << 8)
                    | ((block[j + 2] & 0xff) << 16)
                    | ((block[j + 3] & 0xff) << 24);
            }
            return words;
        }

        private static int[] first8(int[] words) {
            int[] out = new int[8];
            System.arraycopy(words, 0, out, 0, 8);
            return out;
        }

        private static byte[] wordsToBytes(int[] words) {
            byte[] out = new byte[words.length * 4];
            for (int i = 0; i < words.length; i++) {
                int w = words[i];
                int j = i * 4;
                out[j] = (byte) w;
                out[j + 1] = (byte) (w >>> 8);
                out[j + 2] = (byte) (w >>> 16);
                out[j + 3] = (byte) (w >>> 24);
            }
            return out;
        }
    }
}
