package com.provekit.realize;

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
}
