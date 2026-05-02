package com.provekit.lift.cofoja;

import java.util.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.provekit.lift.*;

/**
 * Cofoja extractor with a proper tokenizer + recursive-descent parser.
 *
 * The critical requirement: Cofoja expressions that encode the same
 * runtime constraint as other annotation families MUST produce
 * byte-for-byte identical IR. For example:
 *
 *   @Requires("name != null")
 *
 * must produce the exact same JSON as Bean Validation's @NotNull:
 *
 *   {"kind":"atomic","name":"neq","args":[...]}
 */
public class CofojaExtractor implements Extractor {
    public String name() { return "cofoja"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        for (TypeDeclaration<?> type : cu.getTypes()) {
            for (BodyDeclaration<?> member : type.getMembers()) {
                if (member instanceof MethodDeclaration m) extractMethod(m, out);
            }
        }
        return out;
    }

    private void extractMethod(MethodDeclaration method, List<ContractDecl> out) {
        String symbol = method.getNameAsString();
        List<String> pres = new ArrayList<>(), posts = new ArrayList<>(), invs = new ArrayList<>();
        for (AnnotationExpr ann : method.getAnnotations()) {
            String name = simpleName(ann.getNameAsString());
            switch (name) {
                case "Requires" -> extractString(ann).ifPresent(s -> pres.add(toIr(s)));
                case "Ensures"  -> extractString(ann).ifPresent(s -> posts.add(toIr(s)));
                case "Invariant"-> extractString(ann).ifPresent(s -> invs.add(toIr(s)));
            }
        }
        if (!pres.isEmpty() || !posts.isEmpty() || !invs.isEmpty()) {
            out.add(new ContractDecl(symbol, pres, posts, invs));
        }
    }

    private String simpleName(String fq) {
        int dot = fq.lastIndexOf('.');
        return dot >= 0 ? fq.substring(dot + 1) : fq;
    }

    private Optional<String> extractString(AnnotationExpr ann) {
        if (ann instanceof SingleMemberAnnotationExpr sma) {
            Expression e = sma.getMemberValue();
            if (e instanceof StringLiteralExpr sle) return Optional.of(sle.getValue());
        }
        if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if (p.getNameAsString().equals("value")) {
                    Expression e = p.getValue();
                    if (e instanceof StringLiteralExpr sle) return Optional.of(sle.getValue());
                }
            }
        }
        return Optional.empty();
    }

    private String toIr(String expr) {
        String e = expr.trim();
        try {
            List<Token> tokens = new Tokenizer(e).tokenize();
            String parsed = new Parser(tokens).parse();
            if (parsed != null) return parsed;
        } catch (Exception ex) {
            // fallback
        }
        return "{\"kind\":\"atomic\",\"name\":\"cofoja_predicate\",\"args\":[{\"kind\":\"const\",\"value\":\"" + esc(e) + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}]}";
    }

    private String esc(String s) { return s.replace("\\","\\\\").replace("\"","\\\""); }

    // ═══════════════════════════════════════════════════════════
    //  Tokenizer
    // ═══════════════════════════════════════════════════════════

    enum TokenType { EOF, IDENT, NUMBER, STRING, GE, LE, NE, EQ, GT, LT, AND, OR, LPAREN, RPAREN }

    record Token(TokenType type, String text) {}

    static final class Tokenizer {
        private final String input;
        private int pos;

        Tokenizer(String input) { this.input = input; this.pos = 0; }

        List<Token> tokenize() {
            List<Token> tokens = new ArrayList<>();
            while (pos < input.length()) {
                skipWhitespace();
                if (pos >= input.length()) break;
                char c = input.charAt(pos);
                Token t = switch (c) {
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
                tokens.add(t);
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
            pos++; // consume opening quote
            StringBuilder sb = new StringBuilder();
            while (pos < input.length() && input.charAt(pos) != quote) {
                if (input.charAt(pos) == '\\') {
                    if (pos + 1 >= input.length()) throw new RuntimeException("unterminated string");
                    char next = input.charAt(pos + 1);
                    sb.append(switch (next) {
                        case 'n' -> '\n'; case 't' -> '\t'; case 'r' -> '\r';
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

    // ═══════════════════════════════════════════════════════════
    //  Parser
    // ═══════════════════════════════════════════════════════════

    static final class Parser {
        private final List<Token> tokens;
        private int pos;

        Parser(List<Token> tokens) { this.tokens = tokens; this.pos = 0; }

        String parse() {
            String r = parseOr();
            if (r == null) return null;
            if (current().type() != TokenType.EOF) return null;
            return r;
        }

        private Token current() { return tokens.get(pos); }

        private boolean match(TokenType type) {
            if (current().type() == type) { pos++; return true; }
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
                case GE -> "gte"; case GT -> "gt"; case LE -> "lte";
                case LT -> "lt";   case NE -> "neq"; case EQ -> "eq";
                default -> null;
            };
            if (op != null) {
                match(current().type()); // consume operator
                String right = parsePrimary();
                if (right == null) return null;
                return atom(op, left, right);
            }
            return left;
        }

        private String parsePrimary() {
            Token t = current();
            return switch (t.type()) {
                case IDENT -> {
                    pos++;
                    yield switch (t.text()) {
                        case "null"  -> cNull();
                        case "true"  -> cBool(true);
                        case "false" -> cBool(false);
                        default      -> var_(t.text());
                    };
                }
                case NUMBER -> { pos++; yield cInt(Long.parseLong(t.text())); }
                case STRING -> { pos++; yield cStr(t.text()); }
                case LPAREN -> {
                    pos++;
                    String inner = parseOr();
                    if (inner == null || !match(TokenType.RPAREN)) yield null;
                    yield inner;
                }
                default -> null;
            };
        }

        // IR constructors — same JSON shapes as BeanValidation + JML + SpringWeb
        private String var_(String n)    { return "{\"kind\":\"var\",\"name\":\"" + n + "\"}"; }
        private String cNull()           { return "{\"kind\":\"const\",\"value\":null,\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"}}"; }
        private String cBool(boolean b)  { return "{\"kind\":\"const\",\"value\":" + b + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Bool\"}}"; }
        private String cInt(long v)      { return "{\"kind\":\"const\",\"value\":" + v + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}"; }
        private String cStr(String s)    { return "{\"kind\":\"const\",\"value\":\"" + s.replace("\\","\\\\").replace("\"","\\\"") + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}"; }
        private String atom(String name, String... args) {
            StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\"" + name + "\",\"args\":[");
            for (int i = 0; i < args.length; i++) { if (i > 0) sb.append(","); sb.append(args[i]); }
            sb.append("]}");
            return sb.toString();
        }
    }
}
