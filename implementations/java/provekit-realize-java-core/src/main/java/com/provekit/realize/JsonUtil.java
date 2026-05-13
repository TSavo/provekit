package com.provekit.realize;

import java.util.ArrayList;
import java.util.List;

final class JsonUtil {
    private JsonUtil() {}

    static String escape(String s) {
        if (s == null) return "";
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                case '\b' -> sb.append("\\b");
                case '\f' -> sb.append("\\f");
                default -> {
                    if (c < 0x20) {
                        sb.append(String.format("\\u%04x", (int) c));
                    } else {
                        sb.append(c);
                    }
                }
            }
        }
        return sb.toString();
    }

    static String quoted(String s) {
        return "\"" + escape(s) + "\"";
    }

    static String decodeJsonStringField(String json, String field) {
        String key = "\"" + field + "\"";
        int ki = json.indexOf(key);
        if (ki < 0) return "";
        int pos = ki + key.length();
        while (pos < json.length()
            && (json.charAt(pos) == ':' || json.charAt(pos) == ' ' || json.charAt(pos) == '\t')) {
            pos++;
        }
        if (pos >= json.length() || json.charAt(pos) != '"') return "";
        pos++;

        StringBuilder sb = new StringBuilder();
        while (pos < json.length()) {
            char c = json.charAt(pos);
            if (c == '"') {
                break;
            }
            if (c == '\\') {
                pos++;
                if (pos >= json.length()) break;
                char esc = json.charAt(pos);
                switch (esc) {
                    case '"' -> sb.append('"');
                    case '\\' -> sb.append('\\');
                    case '/' -> sb.append('/');
                    case 'n' -> sb.append('\n');
                    case 'r' -> sb.append('\r');
                    case 't' -> sb.append('\t');
                    case 'b' -> sb.append('\b');
                    case 'f' -> sb.append('\f');
                    case 'u' -> {
                        if (pos + 4 < json.length()) {
                            String hex = json.substring(pos + 1, pos + 5);
                            try {
                                sb.append((char) Integer.parseInt(hex, 16));
                                pos += 4;
                            } catch (NumberFormatException e) {
                                sb.append('u');
                            }
                        }
                    }
                    default -> sb.append(esc);
                }
            } else {
                sb.append(c);
            }
            pos++;
        }
        return sb.toString();
    }

    static String extractId(String json) {
        int i = json.indexOf("\"id\"");
        if (i < 0) return "null";
        int colon = json.indexOf(':', i + 4);
        if (colon < 0) return "null";
        int comma = json.indexOf(',', colon);
        int brace = json.indexOf('}', colon);
        int end = comma >= 0 && (brace < 0 || comma < brace) ? comma : brace;
        if (end < 0) return "null";
        return json.substring(colon + 1, end).trim();
    }

    static String extractMethod(String json) {
        return decodeJsonStringField(json, "method");
    }

    /**
     * Extract the JSON-RPC "params" value as a raw substring.
     * Handles the case where "params" is a JSON object {...}.
     * Returns the object content (including braces), or "{}" if not found.
     */
    static String extractParamsObject(String json) {
        return extractObjectField(json, "params");
    }

    static String extractObjectField(String json, String field) {
        String key = "\"" + field + "\"";
        int ki = json.indexOf(key);
        if (ki < 0) return "{}";
        int pos = ki + key.length();
        while (pos < json.length()
            && (json.charAt(pos) == ':' || json.charAt(pos) == ' ' || json.charAt(pos) == '\t')) {
            pos++;
        }
        if (pos >= json.length() || json.charAt(pos) != '{') return "{}";
        return extractObjectAt(json, pos);
    }

    private static String extractObjectAt(String json, int pos) {
        if (pos >= json.length() || json.charAt(pos) != '{') return "{}";
        int depth = 0;
        int start = pos;
        while (pos < json.length()) {
            char c = json.charAt(pos);
            if (c == '{') depth++;
            else if (c == '}') {
                depth--;
                if (depth == 0) { pos++; break; }
            } else if (c == '"') {
                pos++;
                while (pos < json.length()) {
                    char sc = json.charAt(pos);
                    if (sc == '\\') { pos += 2; continue; }
                    if (sc == '"') break;
                    pos++;
                }
            }
            pos++;
        }
        return json.substring(start, pos);
    }

    static List<String> decodeJsonObjectArray(String json, String field) {
        String key = "\"" + field + "\"";
        int ki = json.indexOf(key);
        if (ki < 0) return List.of();
        int pos = ki + key.length();
        while (pos < json.length()
            && (json.charAt(pos) == ':' || json.charAt(pos) == ' ' || json.charAt(pos) == '\t')) {
            pos++;
        }
        if (pos >= json.length() || json.charAt(pos) != '[') return List.of();
        pos++;

        List<String> result = new ArrayList<>();
        while (pos < json.length() && json.charAt(pos) != ']') {
            char c = json.charAt(pos);
            if (c == '{') {
                String object = extractObjectAt(json, pos);
                result.add(object);
                pos += object.length();
            } else if (c == '"') {
                pos++;
                while (pos < json.length()) {
                    char sc = json.charAt(pos);
                    if (sc == '\\') { pos += 2; continue; }
                    if (sc == '"') break;
                    pos++;
                }
                pos++;
            } else {
                pos++;
            }
        }
        return result;
    }

    /**
     * Decode a JSON array of strings from the given field name.
     * Handles flat one-level arrays of quoted strings only.
     * Returns an empty list if the field is missing or not an array.
     */
    static List<String> decodeJsonStringArray(String json, String field) {
        String key = "\"" + field + "\"";
        int ki = json.indexOf(key);
        if (ki < 0) return List.of();
        int pos = ki + key.length();
        while (pos < json.length()
            && (json.charAt(pos) == ':' || json.charAt(pos) == ' ' || json.charAt(pos) == '\t')) {
            pos++;
        }
        if (pos >= json.length() || json.charAt(pos) != '[') return List.of();
        pos++; // skip '['

        List<String> result = new ArrayList<>();
        while (pos < json.length() && json.charAt(pos) != ']') {
            char c = json.charAt(pos);
            if (c == '"') {
                // parse quoted string
                pos++;
                StringBuilder sb = new StringBuilder();
                while (pos < json.length()) {
                    char ch = json.charAt(pos);
                    if (ch == '"') { pos++; break; }
                    if (ch == '\\') {
                        pos++;
                        if (pos < json.length()) {
                            char esc = json.charAt(pos);
                            switch (esc) {
                                case '"' -> sb.append('"');
                                case '\\' -> sb.append('\\');
                                case 'n' -> sb.append('\n');
                                case 'r' -> sb.append('\r');
                                case 't' -> sb.append('\t');
                                default -> sb.append(esc);
                            }
                        }
                    } else {
                        sb.append(ch);
                    }
                    pos++;
                }
                result.add(sb.toString());
            } else if (c == ',' || c == ' ' || c == '\t' || c == '\n' || c == '\r') {
                pos++;
            } else {
                pos++;
            }
        }
        return result;
    }
}
