package com.provekit.lift.jml;

import java.util.*;
import java.util.regex.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.provekit.lift.*;

/**
 * JML extractor that parses common JML expressions into canonical IR.
 *
 * The critical requirement: JML expressions that encode the same runtime
 * constraint as other annotation families MUST produce byte-for-byte
 * identical IR. For example:
 *
 *   //@ requires name != null
 *
 * must produce the exact same JSON as Bean Validation's @NotNull:
 *
 *   {"kind":"atomic","name":"neq","args":[
 *     {"kind":"var","name":"name"},
 *     {"kind":"const","value":null,"sort":{"kind":"primitive","name":"Ref"}}
 *   ]}
 */
public class JmlExtractor implements Extractor {
    private static final Pattern REQUIRES = Pattern.compile("//@\\s*requires\\s+(.+)");
    private static final Pattern ENSURES  = Pattern.compile("//@\\s*ensures\\s+(.+)");
    private static final Pattern INVARIANT= Pattern.compile("//@\\s*invariant\\s+(.+)");

    public String name() { return "jml"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        String[] lines = rawSource.split("\n");
        Map<Integer,List<String>> pres = new HashMap<>(), posts = new HashMap<>(), invs = new HashMap<>();

        for (int i=0;i<lines.length;i++) {
            String line = lines[i];
            Matcher m;
            if ((m=REQUIRES.matcher(line)).find())  pres.computeIfAbsent(i,k->new ArrayList<>()).add(m.group(1));
            if ((m=ENSURES .matcher(line)).find())  posts.computeIfAbsent(i,k->new ArrayList<>()).add(m.group(1));
            if ((m=INVARIANT.matcher(line)).find()) invs.computeIfAbsent(i,k->new ArrayList<>()).add(m.group(1));
        }

        for (TypeDeclaration<?> type : cu.getTypes()) {
            for (BodyDeclaration<?> member : type.getMembers()) {
                if (member instanceof MethodDeclaration method) {
                    int line = method.getBegin().map(p->p.line).orElse(0);
                    String symbol = method.getNameAsString();
                    List<String> p = gather(line,pres), po = gather(line,posts), inv = gather(line,invs);
                    if (!p.isEmpty()||!po.isEmpty()||!inv.isEmpty()) out.add(new ContractDecl(symbol,p,po,inv));
                }
            }
        }
        return out;
    }

    private List<String> gather(int methodLine, Map<Integer,List<String>> map) {
        List<String> r = new ArrayList<>();
        for (int i=methodLine-1;i>=Math.max(0,methodLine-20);i--) {
            if (map.containsKey(i)) {
                for (String e : map.get(i)) r.add(jmlToIr(e));
                break;
            }
        }
        return r;
    }

    /**
     * Parse a JML expression and emit canonical IR.
     *
     * Uses a hand-written tokenizer + recursive-descent parser (no regex
     * gymnastics on the expression itself). Anything the parser cannot
     * handle falls back to a jml_predicate atom for backward compatibility.
     */
    private String jmlToIr(String expr) {
        String e = expr.trim().replace("\\result","result");
        try {
            List<JmlTokenizer.Token> tokens = new JmlTokenizer(e).tokenize();
            String parsed = new JmlExprParser(tokens).parse();
            if (parsed != null) return parsed;
        } catch (Exception ex) {
            // Fallback to opaque predicate
        }
        return "{\"kind\":\"atomic\",\"name\":\"jml_predicate\",\"args\":[{\"kind\":\"const\",\"value\":\""+esc(e)+"\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}]}";
    }

    private String esc(String s) { return s.replace("\\","\\\\").replace("\"","\\\""); }

    // ======================== Tokenizer ========================

    /** Simple character-scanning tokenizer for JML boolean expressions. */
    static final class JmlTokenizer {
        enum Type {
            EOF, IDENT, NUMBER, STRING,
            GE, LE, NE, EQ, GT, LT,
            AND, OR,
            LPAREN, RPAREN
        }

        record Token(Type type, String text) {}

        private final String input;
        private int pos;

        JmlTokenizer(String input) { this.input = input; this.pos = 0; }

        List<Token> tokenize() {
            List<Token> tokens = new ArrayList<>();
            while (pos < input.length()) {
                skipWhitespace();
                if (pos >= input.length()) break;
                char c = input.charAt(pos);
                Token t = switch (c) {
                    case '(' -> advance(Type.LPAREN, "(");
                    case ')' -> advance(Type.RPAREN, ")");
                    case '&' -> matchPair('&', Type.AND);
                    case '|' -> matchPair('|', Type.OR);
                    case '>' -> matchOpt('=', Type.GE, Type.GT);
                    case '<' -> matchOpt('=', Type.LE, Type.LT);
                    case '!' -> matchOpt('=', Type.NE, null);
                    case '=' -> matchOpt('=', Type.EQ, null);
                    case '"' -> readString('"');
                    case '\'' -> readString('\'');
                    default -> {
                        if (Character.isDigit(c)) yield readNumber();
                        if (Character.isLetter(c) || c == '_') yield readIdent();
                        throw new RuntimeException("Unexpected character '" + c + "' at position " + pos);
                    }
                };
                tokens.add(t);
            }
            tokens.add(new Token(Type.EOF, ""));
            return tokens;
        }

        private void skipWhitespace() {
            while (pos < input.length() && Character.isWhitespace(input.charAt(pos))) pos++;
        }

        private Token advance(Type type, String text) {
            pos += text.length();
            return new Token(type, text);
        }

        private Token matchPair(char expected, Type type) {
            if (pos + 1 < input.length() && input.charAt(pos + 1) == expected) {
                pos += 2;
                return new Token(type, String.valueOf(expected) + expected);
            }
            throw new RuntimeException("Expected '" + expected + "' at position " + pos);
        }

        private Token matchOpt(char maybe, Type yesType, Type noType) {
            if (pos + 1 < input.length() && input.charAt(pos + 1) == maybe) {
                pos += 2;
                char c = input.charAt(pos - 2);
                return new Token(yesType, "" + c + maybe);
            }
            if (noType != null) {
                return advance(noType, String.valueOf(input.charAt(pos)));
            }
            throw new RuntimeException("Unexpected '" + input.charAt(pos) + "' at position " + pos);
        }

        private Token readString(char quote) {
            pos++; // consume opening quote
            StringBuilder sb = new StringBuilder();
            while (pos < input.length() && input.charAt(pos) != quote) {
                if (input.charAt(pos) == '\\') {
                    if (pos + 1 >= input.length()) throw new RuntimeException("Unterminated string");
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
            if (pos >= input.length()) throw new RuntimeException("Unterminated string");
            pos++; // consume closing quote
            return new Token(Type.STRING, sb.toString());
        }

        private Token readNumber() {
            int start = pos;
            while (pos < input.length() && Character.isDigit(input.charAt(pos))) pos++;
            return new Token(Type.NUMBER, input.substring(start, pos));
        }

        private Token readIdent() {
            int start = pos;
            while (pos < input.length() && (Character.isLetterOrDigit(input.charAt(pos)) || input.charAt(pos) == '_')) pos++;
            return new Token(Type.IDENT, input.substring(start, pos));
        }
    }

    // ======================== Parser ========================

    /** Recursive-descent parser that emits canonical IR JSON. */
    static final class JmlExprParser {
        private final List<JmlTokenizer.Token> tokens;
        private int pos;

        JmlExprParser(List<JmlTokenizer.Token> tokens) { this.tokens = tokens; this.pos = 0; }

        /** Parse the token stream into an IR JSON string, or null if it cannot be parsed. */
        String parse() {
            String result = parseOr();
            if (result == null) return null;
            if (current().type() != JmlTokenizer.Type.EOF) return null;
            return result;
        }

        private JmlTokenizer.Token current() { return tokens.get(pos); }

        private boolean match(JmlTokenizer.Type type) {
            if (current().type() == type) { pos++; return true; }
            return false;
        }

        private String parseOr() {
            String left = parseAnd();
            if (left == null) return null;
            while (match(JmlTokenizer.Type.OR)) {
                String right = parseAnd();
                if (right == null) return null;
                left = "{\"kind\":\"or\",\"operands\":[" + left + "," + right + "]}";
            }
            return left;
        }

        private String parseAnd() {
            String left = parseCmp();
            if (left == null) return null;
            while (match(JmlTokenizer.Type.AND)) {
                String right = parseCmp();
                if (right == null) return null;
                left = "{\"kind\":\"and\",\"operands\":[" + left + "," + right + "]}";
            }
            return left;
        }

        private String parseCmp() {
            String left = parsePrimary();
            if (left == null) return null;

            String opName = switch (current().type()) {
                case GE -> "gte";
                case GT -> "gt";
                case LE -> "lte";
                case LT -> "lt";
                case NE -> "neq";
                case EQ -> "eq";
                default -> null;
            };

            if (opName != null) {
                match(current().type()); // consume operator
                String right = parsePrimary();
                if (right == null) return null;
                return atom(opName, left, right);
            }
            return left;
        }

        private String parsePrimary() {
            JmlTokenizer.Token t = current();
            return switch (t.type()) {
                case IDENT -> {
                    consume();
                    yield switch (t.text()) {
                        case "null"  -> cNull();
                        case "true"  -> cBool(true);
                        case "false" -> cBool(false);
                        default      -> var_(t.text());
                    };
                }
                case NUMBER -> {
                    consume();
                    yield cInt(Long.parseLong(t.text()));
                }
                case STRING -> {
                    consume();
                    yield cStr(t.text());
                }
                case LPAREN -> {
                    consume();
                    String inner = parseOr();
                    if (inner == null || !match(JmlTokenizer.Type.RPAREN)) yield null;
                    yield inner;
                }
                default -> null;
            };
        }

        private void consume() { pos++; }

        // IR constructor helpers — emit EXACTLY the same JSON as BeanValidationExtractor
        private String var_(String n) {
            return "{\"kind\":\"var\",\"name\":\"" + n + "\"}";
        }
        private String cNull() {
            return "{\"kind\":\"const\",\"value\":null,\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"}}";
        }
        private String cBool(boolean b) {
            return "{\"kind\":\"const\",\"value\":" + b + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Bool\"}}";
        }
        private String cInt(long v) {
            return "{\"kind\":\"const\",\"value\":" + v + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";
        }
        private String cStr(String s) {
            return "{\"kind\":\"const\",\"value\":\"" + esc(s) + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}";
        }
        private String atom(String name, String... args) {
            StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\"" + name + "\",\"args\":[");
            for (int i = 0; i < args.length; i++) { if (i > 0) sb.append(","); sb.append(args[i]); }
            sb.append("]}");
            return sb.toString();
        }
        private String esc(String s) { return s.replace("\\","\\\\").replace("\"","\\\""); }
    }
}
