// SPDX-License-Identifier: Apache-2.0
//
// Java-native JUnit assertEquals lifter for the Sugar/ProvekIt substrate.
//
// THE LAW: every fact about Java source comes from a com.sun.source.tree.*
// node. No regex, indexOf, split, or any string-scanning of Java source code
// is used here. JSON-RPC wire protocol codec uses indexOf/split on JSON bytes
// only -- not on Java source.
//
// Lifts: assertEquals(<int-literal>, <ident>(<int-literal-args...>)) inside
// @Test methods into the #euf# contract IR, byte-identical with the Rust lifter.
//
// Non-liftable assertion-like calls emit named lift-gap diagnostics.

import com.sun.source.tree.*;
import com.sun.source.util.*;
import javax.tools.*;
import java.io.*;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.stream.*;

public final class JavaTestAssertionsRpc {

    private static final String SURFACE = "java-test-assertions";
    private static final String VERSION = "0.1.0";

    // ──────────────────────────────────────────────────────────────
    // Entry point: JSON-RPC 2.0 over stdin/stdout, one object per line.
    // ──────────────────────────────────────────────────────────────

    public static void main(String[] args) throws Exception {
        BufferedReader in = new BufferedReader(
                new InputStreamReader(System.in, StandardCharsets.UTF_8));
        String line;
        while ((line = in.readLine()) != null) {
            if (line.trim().isEmpty()) continue;
            String id = extractId(line);
            String method = jsonString(line, "method").orElse("");
            String response;
            try {
                response = switch (method) {
                    case "initialize"                  -> ok(id, initializeResult());
                    case "sugar.plugin.kit_declaration" -> ok(id, kitDeclarationResult());
                    case "lift"                        -> ok(id, lift(line));
                    case "shutdown"                    -> ok(id, "null");
                    default -> error(id, -32603, "unknown method: " + method);
                };
            } catch (Exception e) {
                response = error(id, -32603, e.getMessage() == null ? e.toString() : e.getMessage());
            }
            System.out.println(response);
            System.out.flush();
            if ("shutdown".equals(method)) break;
        }
    }

    // ──────────────────────────────────────────────────────────────
    // Protocol responses
    // ──────────────────────────────────────────────────────────────

    private static String initializeResult() {
        return "{\"name\":\"sugar-lift-java-tests\","
             + "\"version\":\"" + VERSION + "\","
             + "\"protocol_version\":\"pep/1.7.0\","
             + "\"capabilities\":{"
             + "\"authoring_surfaces\":[\"" + SURFACE + "\"],"
             + "\"ir_version\":\"v1.1.0\","
             + "\"emits_signed_mementos\":false}}";
    }

    private static String kitDeclarationResult() {
        return "{\"kit\":{\"id\":\"" + SURFACE + "\",\"language\":\"java\",\"version\":\"" + VERSION + "\"},"
             + "\"rpc\":{\"methods\":["
             + "{\"name\":\"initialize\",\"required\":true},"
             + "{\"name\":\"sugar.plugin.kit_declaration\",\"required\":true},"
             + "{\"name\":\"lift\",\"required\":true},"
             + "{\"name\":\"shutdown\",\"required\":false}"
             + "]},"
             + "\"proofResolution\":{\"strategy\":\"junit\"},"
             + "\"effectKinds\":[],\"effectLeaves\":[],\"guardPredicates\":[],"
             + "\"controlCarriers\":[],\"residueCategories\":[]}";
    }

    // ──────────────────────────────────────────────────────────────
    // lift: parse every *.java file via JavacTask, walk @Test methods
    // ──────────────────────────────────────────────────────────────

    private static String lift(String requestJson) throws IOException {
        String workspaceRoot = jsonString(requestJson, "workspace_root").orElse(".");
        Path root = Path.of(workspaceRoot).toAbsolutePath().normalize();
        List<String> sourcePaths = jsonStringArray(requestJson, "source_paths");
        if (sourcePaths.isEmpty()) sourcePaths = List.of(".");

        List<String> files = enumerateJavaFiles(root, sourcePaths);

        List<String> ir = new ArrayList<>();
        List<String> diagnostics = new ArrayList<>();

        JavaCompiler compiler = ToolProvider.getSystemJavaCompiler();
        if (compiler == null) {
            diagnostics.add(diagnostic("<kit>", "<kit>", "<kit>",
                    "no JavaCompiler available (not running under a JDK)"));
            return irDocument(ir, diagnostics);
        }

        for (String rel : files) {
            Path abs = root.resolve(rel).normalize();
            if (!Files.isReadable(abs)) {
                diagnostics.add(diagnostic(rel, null, null, "cannot read file"));
                continue;
            }
            liftFile(compiler, abs, rel, ir, diagnostics);
        }

        return irDocument(ir, diagnostics);
    }

    // ──────────────────────────────────────────────────────────────
    // Per-file lift using javac parse-only tree walk
    // ──────────────────────────────────────────────────────────────

    private static void liftFile(
            JavaCompiler compiler,
            Path abs,
            String rel,
            List<String> ir,
            List<String> diagnostics) throws IOException {

        String source = Files.readString(abs, StandardCharsets.UTF_8);
        // Provide the source as an in-memory JavaFileObject so javac doesn't
        // need the file on its classpath.
        JavaFileObject fo = new StringJavaFileObject(abs.toString(), source);

        StandardJavaFileManager fm = compiler.getStandardFileManager(null, null, StandardCharsets.UTF_8);
        // Parse-only task: no --release flag needed to avoid module access issues.
        JavacTask task = (JavacTask) compiler.getTask(
                null, fm, null,
                List.of("--release", "21"),
                null,
                List.of(fo));

        Iterable<? extends CompilationUnitTree> units;
        try {
            units = task.parse();
        } catch (IOException e) {
            diagnostics.add(diagnostic(rel, null, null, "parse I/O error: " + e.getMessage()));
            fm.close();
            return;
        }

        for (CompilationUnitTree unit : units) {
            // Collect import names so we can recognise fully-qualified @Test
            Set<String> importedNames = collectImports(unit);
            // Walk top-level types
            for (Tree decl : unit.getTypeDecls()) {
                if (decl instanceof ClassTree ct) {
                    walkClassMembers(ct, unit, rel, importedNames, ir, diagnostics, null);
                }
            }
        }
        fm.close();
    }

    // Collect simple import names from the compilation unit (e.g. "Test" from
    // org.junit.Test, "Assertions" from org.junit.jupiter.api.Assertions).
    private static Set<String> collectImports(CompilationUnitTree unit) {
        Set<String> names = new HashSet<>();
        for (ImportTree imp : unit.getImports()) {
            if (imp.isStatic()) continue;
            String name = imp.getQualifiedIdentifier().toString();
            // e.g. org.junit.Test  -> "Test"
            //      org.junit.jupiter.api.Test -> "Test"
            //      org.junit.jupiter.api.Assertions -> "Assertions"
            //      org.junit.Assert -> "Assert"
            int dot = name.lastIndexOf('.');
            if (dot >= 0) names.add(name.substring(dot + 1));
        }
        return names;
    }

    private static void walkClassMembers(
            ClassTree classTree,
            CompilationUnitTree unit,
            String rel,
            Set<String> importedNames,
            List<String> ir,
            List<String> diagnostics,
            String outerClassName) {

        String className = classTree.getSimpleName().toString();
        if (outerClassName != null) className = outerClassName + "." + className;

        for (Tree member : classTree.getMembers()) {
            if (member instanceof MethodTree mt) {
                liftMethod(mt, unit, rel, className, importedNames, ir, diagnostics);
            } else if (member instanceof ClassTree nested) {
                walkClassMembers(nested, unit, rel, importedNames, ir, diagnostics, className);
            }
        }
    }

    // ──────────────────────────────────────────────────────────────
    // Walk a method; only process if annotated @Test
    // ──────────────────────────────────────────────────────────────

    private static void liftMethod(
            MethodTree method,
            CompilationUnitTree unit,
            String rel,
            String className,
            Set<String> importedNames,
            List<String> ir,
            List<String> diagnostics) {

        if (!hasTestAnnotation(method, importedNames)) return;

        String methodName = method.getName().toString();
        String scope = rel + "::" + className + "::" + methodName;

        BlockTree body = method.getBody();
        if (body == null) return;

        for (StatementTree stmt : body.getStatements()) {
            if (stmt instanceof ExpressionStatementTree est) {
                liftStatement(est.getExpression(), scope, ir, diagnostics);
            }
        }
    }

    // ──────────────────────────────────────────────────────────────
    // Determine if a method has @Test (JUnit 4 or 5)
    // ──────────────────────────────────────────────────────────────

    private static boolean hasTestAnnotation(MethodTree method, Set<String> importedNames) {
        for (AnnotationTree ann : method.getModifiers().getAnnotations()) {
            String typeName = ann.getAnnotationType().toString();
            // Accept:  @Test  @org.junit.Test  @org.junit.jupiter.api.Test
            if (typeName.equals("Test")
                    || typeName.equals("org.junit.Test")
                    || typeName.equals("org.junit.jupiter.api.Test")) {
                return true;
            }
        }
        return false;
    }

    // ──────────────────────────────────────────────────────────────
    // Lift or refuse a single expression statement
    // ──────────────────────────────────────────────────────────────

    private static void liftStatement(
            ExpressionTree expr,
            String scope,
            List<String> ir,
            List<String> diagnostics) {

        // Must be a method invocation
        if (!(expr instanceof MethodInvocationTree mit)) return;

        // Resolve the method name: bare name or qualified (Assert.assertEquals, Assertions.assertEquals)
        String methodName = methodInvocationName(mit);
        if (!methodName.equals("assertEquals")) return;

        List<? extends ExpressionTree> args = mit.getArguments();
        if (args.size() < 2) {
            // Can't lift: wrong arity
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    "assertEquals",
                    "assertEquals with arity " + args.size() + " (expected 2 or 3, first string message arg not supported)"));
            return;
        }

        // JUnit's assertEquals(expected, actual) or assertEquals(String msg, expected, actual)
        // We support only the 2-arg form and the 3-arg form where the first arg is NOT an int literal
        // (the message form). Detect message-string form by peeking at arg[0].
        ExpressionTree expectedExpr;
        ExpressionTree actualExpr;
        if (args.size() == 2) {
            expectedExpr = args.get(0);
            actualExpr   = args.get(1);
        } else if (args.size() == 3) {
            // If first arg is a string literal or a non-int-literal, treat as (msg, expected, actual)
            // We refuse by name — Phase 1 doesn't handle 3-arg form
            String item = scopeItem(scope);
            diagnostics.add(diagnostic(scopePath(scope), scopeClass(scope) + "::" + item,
                    "assertEquals",
                    "3-arg assertEquals (message form) not lifted in Phase 1"));
            return;
        } else {
            return; // >3 args: silently skip
        }

        // expected must be an int literal (possibly unary-negated)
        OptionalLong expected = asIntLiteral(expectedExpr);
        if (expected.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    "assertEquals",
                    "expected (first arg) is not an int literal: " + expectedExpr));
            return;
        }

        // actual must be a simple method call with all int-literal args
        if (!(actualExpr instanceof MethodInvocationTree callMit)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    "assertEquals",
                    "actual (second arg) is not a method call: " + actualExpr));
            return;
        }

        // Callee must be a simple identifier (not a method invocation chain)
        String callee = methodInvocationName(callMit);
        if (callee.contains(".")) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    "assertEquals",
                    "callee is qualified (" + callee + "); only bare function names lifted in Phase 1"));
            return;
        }

        List<? extends ExpressionTree> callArgs = callMit.getArguments();
        List<Long> argValues = new ArrayList<>();
        for (ExpressionTree a : callArgs) {
            OptionalLong val = asIntLiteral(a);
            if (val.isEmpty()) {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        "assertEquals",
                        "call arg to " + callee + "(...) is not an int literal: " + a));
                return;
            }
            argValues.add(val.getAsLong());
        }

        // All preconditions met — emit contract
        long expectedVal = expected.getAsLong();
        ir.add(buildContract(callee, argValues, expectedVal));
    }

    // ──────────────────────────────────────────────────────────────
    // Resolve the bare method name from an invocation tree
    // ──────────────────────────────────────────────────────────────

    private static String methodInvocationName(MethodInvocationTree mit) {
        ExpressionTree sel = mit.getMethodSelect();
        if (sel instanceof IdentifierTree id) {
            return id.getName().toString();
        }
        if (sel instanceof MemberSelectTree ms) {
            // e.g. Assert.assertEquals or Assertions.assertEquals
            return ms.getIdentifier().toString();
        }
        return sel.toString();
    }

    // ──────────────────────────────────────────────────────────────
    // Try to read an int literal from an expression, including unary minus.
    // Returns empty if the expression is not a compile-time int literal.
    // ──────────────────────────────────────────────────────────────

    private static OptionalLong asIntLiteral(ExpressionTree expr) {
        // Parenthesized expr
        if (expr instanceof ParenthesizedTree pt) {
            return asIntLiteral(pt.getExpression());
        }
        // Unary minus: -<literal>
        if (expr instanceof UnaryTree ut && ut.getKind() == Tree.Kind.UNARY_MINUS) {
            OptionalLong inner = asIntLiteral(ut.getExpression());
            if (inner.isPresent()) return OptionalLong.of(-inner.getAsLong());
            return OptionalLong.empty();
        }
        // Integer/long literal
        if (expr instanceof LiteralTree lt) {
            Object val = lt.getValue();
            if (val instanceof Integer i) return OptionalLong.of(i);
            if (val instanceof Long l) return OptionalLong.of(l);
        }
        return OptionalLong.empty();
    }

    // ──────────────────────────────────────────────────────────────
    // Build the #euf# contract JSON (byte-identical to Rust lifter)
    //
    // Name: <callee>#euf#c:callresult_<safe>_a<arity>(i:<arg>[,i:<arg>...])::assertion
    // IR shape:
    //   {"kind":"contract","name":"...","outBinding":"out",
    //    "inv":{"kind":"and","operands":[
    //      {"kind":"atomic","name":"=","args":[
    //        {"kind":"ctor","name":"call:<callee>","args":[
    //          {"kind":"const","value":<v>,"sort":{"kind":"primitive","name":"Int"}}...]},
    //        {"kind":"const","value":<expected>,"sort":{"kind":"primitive","name":"Int"}}]}]}}
    // ──────────────────────────────────────────────────────────────

    private static String buildContract(String callee, List<Long> argValues, long expected) {
        String safeName = callee.chars()
                .mapToObj(ch -> (ch < 128 && Character.isLetterOrDigit(ch))
                        ? Character.toString((char) ch) : "_")
                .collect(Collectors.joining());
        int arity = argValues.size();
        String argSig = argValues.stream().map(v -> "i:" + v).collect(Collectors.joining(","));
        String contractName = callee + "#euf#c:callresult_" + safeName + "_a" + arity
                + "(" + argSig + ")::assertion";

        // Build call ctor args
        StringBuilder ctorArgs = new StringBuilder();
        for (int i = 0; i < argValues.size(); i++) {
            if (i > 0) ctorArgs.append(',');
            ctorArgs.append("{\"kind\":\"const\",\"value\":")
                    .append(argValues.get(i))
                    .append(",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}");
        }

        return "{\"kind\":\"contract\""
             + ",\"name\":\"" + esc(contractName) + "\""
             + ",\"outBinding\":\"out\""
             + ",\"inv\":{\"kind\":\"and\",\"operands\":["
             + "{\"kind\":\"atomic\",\"name\":\"=\",\"args\":["
             + "{\"kind\":\"ctor\",\"name\":\"call:" + esc(callee) + "\",\"args\":["
             + ctorArgs
             + "]},"
             + "{\"kind\":\"const\",\"value\":" + expected
             + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}"
             + "]}]}}";
    }

    // ──────────────────────────────────────────────────────────────
    // File enumeration: walks dirs, skips target/build/.git/...
    // ──────────────────────────────────────────────────────────────

    private static List<String> enumerateJavaFiles(Path root, List<String> sourcePaths) throws IOException {
        List<String> out = new ArrayList<>();
        Set<String> IGNORED = Set.of("target", "build", ".git", "node_modules",
                "__pycache__", ".sugar", ".venv", "venv");
        for (String entry : sourcePaths) {
            Path candidate = root.resolve(entry).normalize();
            if (Files.isDirectory(candidate)) {
                try (Stream<Path> stream = Files.walk(candidate)) {
                    stream.filter(Files::isRegularFile)
                          .filter(p -> p.getFileName().toString().endsWith(".java"))
                          .filter(p -> {
                              // Check no path segment is an ignored dir
                              Path rel = root.relativize(p.normalize());
                              for (int i = 0; i < rel.getNameCount() - 1; i++) {
                                  if (IGNORED.contains(rel.getName(i).toString())) return false;
                              }
                              return true;
                          })
                          .map(p -> root.relativize(p.normalize()).toString().replace('\\', '/'))
                          .forEach(out::add);
                }
            } else if (Files.isRegularFile(candidate) && candidate.getFileName().toString().endsWith(".java")) {
                out.add(root.relativize(candidate.normalize()).toString().replace('\\', '/'));
            }
        }
        out.sort(Comparator.naturalOrder());
        return out;
    }

    // ──────────────────────────────────────────────────────────────
    // Helpers: scope parsing, IR document builder, diagnostics
    // ──────────────────────────────────────────────────────────────

    // scope = "path::ClassName::methodName" (or "path::Class.Inner::method")
    private static String scopePath(String scope) {
        int i = scope.indexOf("::");
        return i >= 0 ? scope.substring(0, i) : scope;
    }
    private static String scopeClass(String scope) {
        int i = scope.indexOf("::");
        int j = scope.lastIndexOf("::");
        if (i < 0) return scope;
        if (i == j) return scope.substring(i + 2);
        return scope.substring(i + 2, j);
    }
    private static String scopeItem(String scope) {
        int j = scope.lastIndexOf("::");
        return j >= 0 ? scope.substring(j + 2) : scope;
    }
    private static String scopeClassMethod(String scope) {
        int i = scope.indexOf("::");
        return i >= 0 ? scope.substring(i + 2) : scope;
    }

    private static String irDocument(List<String> ir, List<String> diagnostics) {
        return "{\"kind\":\"ir-document\""
             + ",\"ir\":[" + String.join(",", ir) + "]"
             + ",\"diagnostics\":[" + String.join(",", diagnostics) + "]"
             + ",\"refusals\":[]}";
    }

    private static String diagnostic(String path, String item, String detail, String reason) {
        String itemField = item != null
                ? ",\"item\":\"" + esc(item + (detail != null ? "/" + detail : "")) + "\""
                : "";
        return "{\"kind\":\"lift-gap\""
             + ",\"path\":\"" + esc(path) + "\""
             + itemField
             + ",\"reason\":\"" + esc(reason) + "\"}";
    }

    // ──────────────────────────────────────────────────────────────
    // Minimal JSON-RPC wire codec (operates on the JSON wire bytes,
    // NOT on Java source — this is correct and lawful).
    // ──────────────────────────────────────────────────────────────

    private static Optional<String> jsonString(String json, String key) {
        // Find the first occurrence of "key":
        int keyPos = json.indexOf("\"" + key + "\"");
        if (keyPos < 0) return Optional.empty();
        int colon = json.indexOf(':', keyPos + key.length() + 2);
        if (colon < 0) return Optional.empty();
        int i = colon + 1;
        while (i < json.length() && Character.isWhitespace(json.charAt(i))) i++;
        if (i >= json.length() || json.charAt(i) != '"') return Optional.empty();
        StringBuilder out = new StringBuilder();
        boolean escaped = false;
        for (int j = i + 1; j < json.length(); j++) {
            char ch = json.charAt(j);
            if (escaped) {
                out.append(switch (ch) {
                    case 'n' -> '\n'; case 'r' -> '\r'; case 't' -> '\t';
                    case '"' -> '"'; case '\\' -> '\\'; default -> ch;
                });
                escaped = false;
            } else if (ch == '\\') { escaped = true; }
            else if (ch == '"') { return Optional.of(out.toString()); }
            else { out.append(ch); }
        }
        return Optional.empty();
    }

    private static List<String> jsonStringArray(String json, String key) {
        int keyPos = json.indexOf("\"" + key + "\"");
        if (keyPos < 0) return List.of();
        int start = json.indexOf('[', keyPos);
        if (start < 0) return List.of();
        int end = matchingBracket(json, start, '[', ']');
        if (end < 0) return List.of();
        String body = json.substring(start + 1, end);
        List<String> out = new ArrayList<>();
        int i = 0;
        while (i < body.length()) {
            while (i < body.length() && Character.isWhitespace(body.charAt(i))) i++;
            if (i >= body.length()) break;
            if (body.charAt(i) == '"') {
                StringBuilder sb = new StringBuilder();
                boolean esc = false;
                i++;
                while (i < body.length()) {
                    char ch = body.charAt(i++);
                    if (esc) {
                        sb.append(switch (ch) {
                            case 'n' -> '\n'; case 'r' -> '\r'; case 't' -> '\t';
                            case '"' -> '"'; case '\\' -> '\\'; default -> ch;
                        });
                        esc = false;
                    } else if (ch == '\\') { esc = true; }
                    else if (ch == '"') { out.add(sb.toString()); break; }
                    else { sb.append(ch); }
                }
            } else { i++; }
        }
        return out;
    }

    private static int matchingBracket(String s, int open, char openCh, char closeCh) {
        int depth = 0;
        boolean inStr = false;
        boolean esc = false;
        for (int i = open; i < s.length(); i++) {
            char ch = s.charAt(i);
            if (inStr) {
                if (esc) esc = false;
                else if (ch == '\\') esc = true;
                else if (ch == '"') inStr = false;
            } else if (ch == '"') inStr = true;
            else if (ch == openCh) depth++;
            else if (ch == closeCh) { if (--depth == 0) return i; }
        }
        return -1;
    }

    // Extract id field for JSON-RPC reply: returns the raw JSON value (string or number or null)
    private static String extractId(String json) {
        int keyPos = json.indexOf("\"id\"");
        if (keyPos < 0) return "null";
        int colon = json.indexOf(':', keyPos);
        if (colon < 0) return "null";
        int i = colon + 1;
        while (i < json.length() && Character.isWhitespace(json.charAt(i))) i++;
        if (i >= json.length()) return "null";
        if (json.charAt(i) == '"') {
            Optional<String> v = jsonString(json.substring(keyPos), "id");
            return v.map(s -> "\"" + esc(s) + "\"").orElse("null");
        }
        int start = i;
        while (i < json.length() && json.charAt(i) != ',' && json.charAt(i) != '}') i++;
        return json.substring(start, i).trim();
    }

    private static String ok(String id, String result) {
        return "{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"result\":" + result + "}";
    }

    private static String error(String id, int code, String msg) {
        return "{\"jsonrpc\":\"2.0\",\"id\":" + id
             + ",\"error\":{\"code\":" + code + ",\"message\":\"" + esc(msg) + "\"}}";
    }

    private static String esc(String s) {
        return s.replace("\\", "\\\\").replace("\"", "\\\"")
                .replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t");
    }

    // ──────────────────────────────────────────────────────────────
    // In-memory JavaFileObject for parse-only compilation
    // ──────────────────────────────────────────────────────────────

    private static final class StringJavaFileObject extends SimpleJavaFileObject {
        private final String content;
        StringJavaFileObject(String path, String content) {
            super(URI.create("file:///" + path.replace('\\', '/').replace(" ", "%20")),
                  Kind.SOURCE);
            this.content = content;
        }
        @Override public CharSequence getCharContent(boolean ignoreEncodingErrors) {
            return content;
        }
    }
}
