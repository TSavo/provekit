package com.provekit.lift;

import java.util.*;

public final class ContractExpressionParser {
    private ContractExpressionParser() {}

    public static String parseOrFallback(String expression, String fallbackAtomName) {
        String trimmed = expression.trim();
        try {
            List<Token> tokens = new Tokenizer(trimmed).tokenize();
            String parsed = new Parser(tokens).parse();
            if (parsed != null) return parsed;
        } catch (Exception ex) {
            // Fall through to an opaque predicate for expressions outside this subset.
        }
        return "{\"kind\":\"atomic\",\"name\":\"" + fallbackAtomName + "\",\"args\":[{\"kind\":\"const\",\"value\":\""
            + esc(trimmed) + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}]}";
    }

    private enum TokenType { EOF, IDENT, NUMBER, STRING, GE, LE, NE, EQ, GT, LT, AND, OR, LPAREN, RPAREN }

    private record Token(TokenType type, String text) {}

    private static final class Tokenizer {
        private final String input;
        private int pos;

        Tokenizer(String input) {
            this.input = input;
            this.pos = 0;
        }

        List<Token> tokenize() {
            List<Token> tokens = new ArrayList<>();
            while (pos < input.length()) {
                skipWhitespace();
                if (pos >= input.length()) break;
                char c = input.charAt(pos);
                Token token = switch (c) {
                    case '(' -> advance(TokenType.LPAREN, "(");
                    case ')' -> advance(TokenType.RPAREN, ")");
                    case '&' -> matchPair('&', TokenType.AND);
                    case '|' -> matchPair('|', TokenType.OR);
                    case '>' -> matchOpt('=', TokenType.GE, TokenType.GT);
                    case '<' -> matchOpt('=', TokenType.LE, TokenType.LT);
                    case '!' -> matchOpt('=', TokenType.NE, null);
                    case '=' -> matchOpt('=', TokenType.EQ, null);
                    case '"' -> readString('"');
                    case '\'' -> readString('\'');
                    default -> {
                        if (Character.isDigit(c)) yield readNumber();
                        if (Character.isLetter(c) || c == '_') yield readIdent();
                        throw new RuntimeException("unexpected '" + c + "' at " + pos);
                    }
                };
                tokens.add(token);
            }
            tokens.add(new Token(TokenType.EOF, ""));
            return tokens;
        }

        private void skipWhitespace() {
            while (pos < input.length() && Character.isWhitespace(input.charAt(pos))) pos++;
        }

        private Token advance(TokenType type, String text) {
            pos += text.length();
            return new Token(type, text);
        }

        private Token matchPair(char expected, TokenType type) {
            if (pos + 1 < input.length() && input.charAt(pos + 1) == expected) {
                pos += 2;
                char c = input.charAt(pos - 2);
                return new Token(type, "" + c + expected);
            }
            throw new RuntimeException("expected '" + expected + "' at " + pos);
        }

        private Token matchOpt(char maybe, TokenType yes, TokenType no) {
            if (pos + 1 < input.length() && input.charAt(pos + 1) == maybe) {
                pos += 2;
                char c = input.charAt(pos - 2);
                return new Token(yes, "" + c + maybe);
            }
            if (no != null) return advance(no, String.valueOf(input.charAt(pos)));
            throw new RuntimeException("unexpected '" + input.charAt(pos) + "' at " + pos);
        }

        private Token readString(char quote) {
            pos++;
            StringBuilder sb = new StringBuilder();
            while (pos < input.length() && input.charAt(pos) != quote) {
                if (input.charAt(pos) == '\\') {
                    if (pos + 1 >= input.length()) throw new RuntimeException("unterminated string");
                    char next = input.charAt(pos + 1);
                    sb.append(switch (next) {
                        case 'n' -> '\n';
                        case 't' -> '\t';
                        case 'r' -> '\r';
                        case '\\', '"', '\'' -> next;
                        default -> next;
                    });
                    pos += 2;
                } else {
                    sb.append(input.charAt(pos++));
                }
            }
            if (pos >= input.length()) throw new RuntimeException("unterminated string");
            pos++;
            return new Token(TokenType.STRING, sb.toString());
        }

        private Token readNumber() {
            int start = pos;
            while (pos < input.length() && Character.isDigit(input.charAt(pos))) pos++;
            return new Token(TokenType.NUMBER, input.substring(start, pos));
        }

        private Token readIdent() {
            int start = pos;
            while (pos < input.length() && (Character.isLetterOrDigit(input.charAt(pos)) || input.charAt(pos) == '_')) pos++;
            return new Token(TokenType.IDENT, input.substring(start, pos));
        }
    }

    private static final class Parser {
        private final List<Token> tokens;
        private int pos;

        Parser(List<Token> tokens) {
            this.tokens = tokens;
            this.pos = 0;
        }

        String parse() {
            String result = parseOr();
            if (result == null) return null;
            if (current().type() != TokenType.EOF) return null;
            return result;
        }

        private Token current() { return tokens.get(pos); }

        private boolean match(TokenType type) {
            if (current().type() == type) {
                pos++;
                return true;
            }
            return false;
        }

        private String parseOr() {
            String left = parseAnd();
            if (left == null) return null;
            while (match(TokenType.OR)) {
                String right = parseAnd();
                if (right == null) return null;
                left = "{\"kind\":\"or\",\"operands\":[" + left + "," + right + "]}";
            }
            return left;
        }

        private String parseAnd() {
            String left = parseCmp();
            if (left == null) return null;
            while (match(TokenType.AND)) {
                String right = parseCmp();
                if (right == null) return null;
                left = "{\"kind\":\"and\",\"operands\":[" + left + "," + right + "]}";
            }
            return left;
        }

        private String parseCmp() {
            String left = parsePrimary();
            if (left == null) return null;
            String op = switch (current().type()) {
                case GE -> "gte";
                case GT -> "gt";
                case LE -> "lte";
                case LT -> "lt";
                case NE -> "neq";
                case EQ -> "eq";
                default -> null;
            };
            if (op != null) {
                match(current().type());
                String right = parsePrimary();
                if (right == null) return null;
                return atom(op, left, right);
            }
            return left;
        }

        private String parsePrimary() {
            Token token = current();
            return switch (token.type()) {
                case IDENT -> {
                    pos++;
                    yield switch (token.text()) {
                        case "null" -> cNull();
                        case "true" -> cBool(true);
                        case "false" -> cBool(false);
                        default -> var_(token.text());
                    };
                }
                case NUMBER -> {
                    pos++;
                    yield cInt(Long.parseLong(token.text()));
                }
                case STRING -> {
                    pos++;
                    yield cStr(token.text());
                }
                case LPAREN -> {
                    pos++;
                    String inner = parseOr();
                    if (inner == null || !match(TokenType.RPAREN)) yield null;
                    yield inner;
                }
                default -> null;
            };
        }

        private String var_(String n) { return "{\"kind\":\"var\",\"name\":\"" + n + "\"}"; }
        private String cNull() { return "{\"kind\":\"const\",\"value\":null,\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"}}"; }
        private String cBool(boolean b) { return "{\"kind\":\"const\",\"value\":" + b + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Bool\"}}"; }
        private String cInt(long v) { return "{\"kind\":\"const\",\"value\":" + v + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}"; }
        private String cStr(String s) { return "{\"kind\":\"const\",\"value\":\"" + esc(s) + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}"; }
        private String atom(String name, String... args) {
            StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\"" + name + "\",\"args\":[");
            for (int i = 0; i < args.length; i++) {
                if (i > 0) sb.append(",");
                sb.append(args[i]);
            }
            sb.append("]}");
            return sb.toString();
        }
    }

    private static String esc(String s) {
        return s.replace("\\", "\\\\").replace("\"", "\\\"");
    }
}
