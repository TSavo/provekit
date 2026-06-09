// SPDX-License-Identifier: Apache-2.0
//
// Java Panama FFM call-edge lifter.
//
// This is the Java analogue of sugar_lift_py_tests/cpython_ctypes_resolver.py:
// it reads Java source, detects supported FFM downcall sites, and emits
// call-edge declarations keyed to the native target symbol. It does not mint
// contracts itself; it runs as a consumer lift surface and uses the forwarded
// contract_bindings to pin the Java assertion row and to prove the rust #euf#
// target row exists.

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Optional;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import java.util.stream.Stream;

public final class PanamaFfmLiftRpc {
    private static final String SURFACE = "java-panama-ffm";
    private static final Pattern LIBRARY_LOOKUP = Pattern.compile(
            "SymbolLookup\\s*\\.\\s*libraryLookup\\s*\\(\\s*\"([^\"]+)\"");
    private static final Pattern SYMBOL_FIND = Pattern.compile("\\.\\s*find\\s*\\(\\s*\"([^\"]+)\"\\s*\\)");
    private static final Pattern ASSERT_EQUALS = Pattern.compile(
            "\\bassertEquals\\s*\\(\\s*(-?\\d+)\\s*,\\s*([A-Za-z_$][A-Za-z0-9_$]*)\\s*\\(\\s*(-?\\d+)\\s*\\)\\s*\\)");
    private static final Pattern METHOD_HEADER = Pattern.compile(
            "\\b(?:public|private|protected|static|final|void|int|long|throws|[A-Za-z_$][A-Za-z0-9_$.<>\\[\\]]+|\\s)+\\s+([A-Za-z_$][A-Za-z0-9_$]*)\\s*\\([^)]*\\)\\s*(?:throws\\s+[A-Za-z0-9_$. ,]+)?\\s*\\{");

    private PanamaFfmLiftRpc() {}

    public static void main(String[] args) throws Exception {
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        String line;
        while ((line = in.readLine()) != null) {
            if (line.trim().isEmpty()) {
                continue;
            }
            String id = extractId(line);
            String method = jsonString(line, "method").orElse("");
            String response;
            try {
                response = switch (method) {
                    case "initialize" -> ok(id, initializeResult());
                    case "sugar.plugin.kit_declaration" -> ok(id, kitDeclarationResult());
                    case "lift" -> ok(id, lift(line));
                    case "shutdown", "sugar.plugin.shutdown" -> ok(id, "null");
                    default -> error(id, -32603, "unknown method: " + method);
                };
            } catch (Exception e) {
                response = error(id, -32603, e.getMessage() == null ? e.toString() : e.getMessage());
            }
            System.out.println(response);
            System.out.flush();
            if ("shutdown".equals(method) || "sugar.plugin.shutdown".equals(method)) {
                break;
            }
        }
    }

    private static String initializeResult() {
        return "{"
                + "\"name\":\"sugar-lift-java-panama-ffm\","
                + "\"version\":\"0.1.0\","
                + "\"protocol_version\":\"pep/1.7.0\","
                + "\"capabilities\":{"
                + "\"authoring_surfaces\":[\"" + SURFACE + "\"],"
                + "\"ir_version\":\"v1.1.0\","
                + "\"emits_signed_mementos\":false"
                + "}"
                + "}";
    }

    private static String kitDeclarationResult() {
        return "{"
                + "\"kit\":{\"id\":\"" + SURFACE + "\",\"language\":\"java\",\"version\":\"0.1.0\"},"
                + "\"rpc\":{\"methods\":["
                + "{\"name\":\"initialize\",\"required\":true},"
                + "{\"name\":\"sugar.plugin.kit_declaration\",\"required\":true},"
                + "{\"name\":\"lift\",\"required\":true},"
                + "{\"name\":\"shutdown\",\"required\":false}"
                + "]},"
                + "\"proofResolution\":{\"strategy\":\"panama-ffm-call-edge\"},"
                + "\"effectKinds\":[],\"effectLeaves\":[],\"guardPredicates\":[],"
                + "\"controlCarriers\":[],\"residueCategories\":[]"
                + "}";
    }

    private static String lift(String requestJson) throws IOException {
        String workspaceRoot = jsonString(requestJson, "workspace_root").orElse(".");
        Path root = Path.of(workspaceRoot);
        List<String> sourcePaths = jsonStringArray(requestJson, "source_paths");
        if (sourcePaths.isEmpty()) {
            sourcePaths = List.of(".");
        }
        List<Binding> bindings = parseContractBindings(requestJson);
        List<String> files = enumerateJavaFiles(root, sourcePaths);
        List<String> edges = new ArrayList<>();
        List<String> diagnostics = new ArrayList<>();
        Set<String> seen = new HashSet<>();

        for (String rel : files) {
            Path path = root.resolve(rel);
            String source = Files.readString(path, StandardCharsets.UTF_8);
            ScanResult scan = scanSource(source);
            if (scan.kit == null || scan.symbols.isEmpty()) {
                continue;
            }
            String currentMethod = "<unknown>";
            String[] lines = source.split("\\R", -1);
            for (int i = 0; i < lines.length; i++) {
                String text = lines[i];
                Matcher methodMatcher = METHOD_HEADER.matcher(text);
                if (methodMatcher.find()) {
                    currentMethod = methodMatcher.group(1);
                }
                Matcher assertion = ASSERT_EQUALS.matcher(text);
                while (assertion.find()) {
                    String callee = assertion.group(2);
                    if (!scan.symbols.contains(callee)) {
                        continue;
                    }
                    String arg = assertion.group(3);
                    String eufName = eufAssertionName(callee, arg);
                    Optional<Binding> sourceBinding = bindings.stream()
                            .filter(b -> b.name.equals(eufName) && b.targetProofCid == null)
                            .findFirst();
                    Optional<Binding> targetBinding = bindings.stream()
                            .filter(b -> b.name.equals(eufName) && b.targetProofCid != null)
                            .findFirst();
                    if (sourceBinding.isEmpty()) {
                        diagnostics.add(diagnostic(rel, "missing-source-contract-binding", eufName));
                        continue;
                    }
                    if (targetBinding.isEmpty()) {
                        diagnostics.add(diagnostic(rel, "missing-target-contract-binding", eufName));
                        continue;
                    }
                    int line = i + 1;
                    int column = assertion.start(2) + 1;
                    String targetSymbol = scan.kit + ":" + eufName;
                    String edge = callEdgeJson(
                            sourceBinding.get().cid,
                            targetBinding.get().cid,
                            targetSymbol,
                            rel,
                            line,
                            column,
                            currentMethod);
                    String key = sourceBinding.get().cid + "\n" + targetSymbol + "\n" + rel + "\n" + line + "\n" + column;
                    if (seen.add(key)) {
                        edges.add(edge);
                    }
                }
            }
        }

        edges.sort(Comparator.naturalOrder());
        Path sidecar = root.resolve("java-panama-ffm.call-edges.json");
        Files.writeString(sidecar, "{\"edges\":[" + String.join(",", edges) + "]}\n", StandardCharsets.UTF_8);

        return "{"
                + "\"kind\":\"ir-document\","
                + "\"ir\":[" + String.join(",", edges) + "],"
                + "\"callEdges\":[" + String.join(",", edges) + "],"
                + "\"diagnostics\":[" + String.join(",", diagnostics) + "],"
                + "\"refusals\":[]"
                + "}";
    }

    private static ScanResult scanSource(String source) {
        Matcher libMatcher = LIBRARY_LOOKUP.matcher(source);
        String kit = null;
        if (libMatcher.find()) {
            kit = resolveKit(stripLibraryName(libMatcher.group(1)));
        }
        Set<String> symbols = new HashSet<>();
        Matcher symbolMatcher = SYMBOL_FIND.matcher(source);
        while (symbolMatcher.find()) {
            symbols.add(symbolMatcher.group(1));
        }
        return new ScanResult(kit, symbols);
    }

    private static List<String> enumerateJavaFiles(Path root, List<String> sourcePaths) throws IOException {
        List<String> out = new ArrayList<>();
        for (String entry : sourcePaths) {
            Path path = root.resolve(entry).normalize();
            if (Files.isDirectory(path)) {
                try (Stream<Path> stream = Files.walk(path)) {
                    stream.filter(Files::isRegularFile)
                            .filter(p -> p.getFileName().toString().endsWith(".java"))
                            .filter(p -> !isIgnoredWorkspacePath(root, p))
                            .forEach(p -> out.add(root.relativize(p).toString().replace('\\', '/')));
                }
            } else if (Files.isRegularFile(path) && path.getFileName().toString().endsWith(".java")) {
                out.add(root.relativize(path).toString().replace('\\', '/'));
            }
        }
        out.sort(Comparator.naturalOrder());
        return out;
    }

    private static boolean isIgnoredWorkspacePath(Path root, Path path) {
        String rel = root.relativize(path).toString().replace('\\', '/');
        return rel.startsWith("target/") || rel.contains("/target/")
                || rel.startsWith(".sugar/") || rel.contains("/.sugar/");
    }

    private static String stripLibraryName(String raw) {
        String name = raw.replace('\\', '/');
        int slash = name.lastIndexOf('/');
        if (slash >= 0) {
            name = name.substring(slash + 1);
        }
        while (true) {
            String lower = name.toLowerCase();
            if (lower.endsWith(".so") || lower.endsWith(".dll") || lower.endsWith(".dylib") || lower.endsWith(".a")) {
                name = name.substring(0, name.lastIndexOf('.'));
            } else {
                int dot = name.lastIndexOf('.');
                if (dot > 0 && name.substring(dot + 1).chars().allMatch(Character::isDigit)) {
                    name = name.substring(0, dot);
                } else {
                    break;
                }
            }
        }
        if (name.startsWith("lib")) {
            name = name.substring(3);
        }
        return name;
    }

    private static String resolveKit(String libName) {
        if (libName == null || libName.isBlank()) {
            return null;
        }
        return switch (libName) {
            case "c", "m", "pthread", "dl", "rt", "z", "ssl", "crypto" -> "libc-system";
            default -> "rust-kit";
        };
    }

    private static String eufAssertionName(String callee, String arg) {
        String safe = callee.chars()
                .mapToObj(ch -> Character.isLetterOrDigit(ch) && ch < 128 ? Character.toString((char) ch) : "_")
                .reduce("", String::concat);
        return callee + "#euf#c:callresult_" + safe + "_a1(i:" + arg + ")::assertion";
    }

    private static String callEdgeJson(
            String sourceCid,
            String targetCid,
            String targetSymbol,
            String file,
            int line,
            int column,
            String caller) {
        return "{"
                + "\"callSiteLocus\":{\"column\":" + column + ",\"file\":\"" + esc(file) + "\",\"line\":" + line + "},"
                + "\"evidenceTerm\":{\"args\":[{\"kind\":\"var\",\"name\":\"" + esc(caller) + "\"}],\"kind\":\"atomic\",\"name\":\"call-site-obligation\"},"
                + "\"kind\":\"call-edge\","
                + "\"schemaVersion\":\"1\","
                + "\"sourceContractCid\":\"" + esc(sourceCid) + "\","
                + "\"targetContractCid\":\"" + esc(targetCid) + "\","
                + "\"targetSymbol\":\"" + esc(targetSymbol) + "\""
                + "}";
    }

    private static String diagnostic(String path, String reason, String detail) {
        return "{"
                + "\"kind\":\"lift-gap\","
                + "\"path\":\"" + esc(path) + "\","
                + "\"reason\":\"" + esc(reason + ": " + detail) + "\""
                + "}";
    }

    private static List<Binding> parseContractBindings(String json) {
        List<Binding> out = new ArrayList<>();
        for (String object : objectArray(json, "contract_bindings")) {
            Optional<String> name = jsonString(object, "name");
            Optional<String> cid = jsonString(object, "contract_cid");
            if (name.isEmpty() || cid.isEmpty()) {
                continue;
            }
            out.add(new Binding(name.get(), cid.get(), jsonString(object, "target_proof_cid").orElse(null)));
        }
        return out;
    }

    private static List<String> jsonStringArray(String json, String key) {
        int keyPos = json.indexOf("\"" + key + "\"");
        if (keyPos < 0) {
            return List.of();
        }
        int start = json.indexOf('[', keyPos);
        if (start < 0) {
            return List.of();
        }
        int end = matching(json, start, '[', ']');
        if (end < 0) {
            return List.of();
        }
        String body = json.substring(start + 1, end);
        List<String> out = new ArrayList<>();
        Matcher m = Pattern.compile("\"((?:\\\\.|[^\"])*)\"").matcher(body);
        while (m.find()) {
            out.add(unesc(m.group(1)));
        }
        return out;
    }

    private static List<String> objectArray(String json, String key) {
        int keyPos = json.indexOf("\"" + key + "\"");
        if (keyPos < 0) {
            return List.of();
        }
        int start = json.indexOf('[', keyPos);
        if (start < 0) {
            return List.of();
        }
        int end = matching(json, start, '[', ']');
        if (end < 0) {
            return List.of();
        }
        String body = json.substring(start + 1, end);
        List<String> out = new ArrayList<>();
        int idx = 0;
        while (idx < body.length()) {
            int open = body.indexOf('{', idx);
            if (open < 0) {
                break;
            }
            int close = matching(body, open, '{', '}');
            if (close < 0) {
                break;
            }
            out.add(body.substring(open, close + 1));
            idx = close + 1;
        }
        return out;
    }

    private static Optional<String> jsonString(String json, String key) {
        int keyPos = json.indexOf("\"" + key + "\"");
        if (keyPos < 0) {
            return Optional.empty();
        }
        int colon = json.indexOf(':', keyPos);
        if (colon < 0) {
            return Optional.empty();
        }
        int quote = json.indexOf('"', colon + 1);
        if (quote < 0) {
            return Optional.empty();
        }
        StringBuilder out = new StringBuilder();
        boolean escaped = false;
        for (int i = quote + 1; i < json.length(); i++) {
            char ch = json.charAt(i);
            if (escaped) {
                out.append(switch (ch) {
                    case 'n' -> '\n';
                    case 'r' -> '\r';
                    case 't' -> '\t';
                    case '"' -> '"';
                    case '\\' -> '\\';
                    default -> ch;
                });
                escaped = false;
            } else if (ch == '\\') {
                escaped = true;
            } else if (ch == '"') {
                return Optional.of(out.toString());
            } else {
                out.append(ch);
            }
        }
        return Optional.empty();
    }

    private static int matching(String s, int open, char openCh, char closeCh) {
        int depth = 0;
        boolean inString = false;
        boolean escaped = false;
        for (int i = open; i < s.length(); i++) {
            char ch = s.charAt(i);
            if (inString) {
                if (escaped) {
                    escaped = false;
                } else if (ch == '\\') {
                    escaped = true;
                } else if (ch == '"') {
                    inString = false;
                }
                continue;
            }
            if (ch == '"') {
                inString = true;
            } else if (ch == openCh) {
                depth++;
            } else if (ch == closeCh) {
                depth--;
                if (depth == 0) {
                    return i;
                }
            }
        }
        return -1;
    }

    private static String extractId(String json) {
        int keyPos = json.indexOf("\"id\"");
        if (keyPos < 0) {
            return "null";
        }
        int colon = json.indexOf(':', keyPos);
        if (colon < 0) {
            return "null";
        }
        int i = colon + 1;
        while (i < json.length() && Character.isWhitespace(json.charAt(i))) {
            i++;
        }
        if (i >= json.length()) {
            return "null";
        }
        if (json.charAt(i) == '"') {
            Optional<String> id = jsonString(json.substring(keyPos), "id");
            return id.map(value -> "\"" + esc(value) + "\"").orElse("null");
        }
        int start = i;
        while (i < json.length()) {
            char ch = json.charAt(i);
            if (ch == ',' || ch == '}') {
                break;
            }
            i++;
        }
        return json.substring(start, i).trim();
    }

    private static String ok(String id, String result) {
        return "{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"result\":" + result + "}";
    }

    private static String error(String id, int code, String message) {
        return "{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"error\":{\"code\":" + code
                + ",\"message\":\"" + esc(message) + "\"}}";
    }

    private static String esc(String s) {
        return s.replace("\\", "\\\\")
                .replace("\"", "\\\"")
                .replace("\n", "\\n")
                .replace("\r", "\\r")
                .replace("\t", "\\t");
    }

    private static String unesc(String s) {
        StringBuilder out = new StringBuilder();
        boolean escaped = false;
        for (int i = 0; i < s.length(); i++) {
            char ch = s.charAt(i);
            if (escaped) {
                out.append(switch (ch) {
                    case 'n' -> '\n';
                    case 'r' -> '\r';
                    case 't' -> '\t';
                    case '"' -> '"';
                    case '\\' -> '\\';
                    default -> ch;
                });
                escaped = false;
            } else if (ch == '\\') {
                escaped = true;
            } else {
                out.append(ch);
            }
        }
        return out.toString();
    }

    private record Binding(String name, String cid, String targetProofCid) {}

    private record ScanResult(String kit, Set<String> symbols) {}
}
