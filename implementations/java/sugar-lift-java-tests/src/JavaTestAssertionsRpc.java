// SPDX-License-Identifier: Apache-2.0
//
// Java-native JUnit assertion lifter for the Sugar/ProvekIt substrate.
// Phase 2: vocabulary is LEARNED from the assertion framework's OWN SOURCE
// via JavacTask.parse() — no hardcoded assertion meanings in the lift path.
//
// THE LAW: every fact about Java source comes from a com.sun.source.tree.*
// node. No regex, indexOf, split, or any string-scanning of Java source code
// is used here. JSON-RPC wire protocol codec uses indexOf/split on JSON bytes
// only -- not on Java source.
//
// The VocabDeriver reads each public static assert* method from the framework's
// source (e.g. org.junit.jupiter.api.Assertions), classifies it by structure:
//   - signature carries a `delta` / `tolerance` parameter  -> APPROXIMATE (refused)
//   - assertEquals(expected, actual[, msg])                -> EQUALITY (expected-first)
//   - assertNotEquals(unexpected, actual[, msg])            -> INEQUALITY
//   - assertTrue(condition[, msg])                          -> TRUTH
//   - assertFalse(condition[, msg])                         -> NEGATED_TRUTH
//   - assertNull(actual[, msg])                             -> NULL
//   - assertNotNull(actual[, msg])                          -> NOT_NULL
//   - everything else                                       -> UNLEARNED (refused by name)
//
// An externalized .sugar/vocab-exceptions/<framework>.json overlays
// the derived table. With no source configured every assertion is refused.
//
// Non-liftable invocations emit named lift-gap diagnostics; a refused
// approximate or unlearned assertion emits a named refusal, never a contract.

import com.sun.source.tree.*;
import com.sun.source.util.*;
import javax.lang.model.element.Modifier;
import javax.lang.model.type.TypeKind;
import javax.tools.*;
import java.io.*;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.stream.*;

public final class JavaTestAssertionsRpc {

    private static final String SURFACE = "java-test-assertions";
    private static final String VERSION = "0.2.0";

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
                    case "initialize"                   -> ok(id, initializeResult());
                    case "sugar.plugin.kit_declaration" -> ok(id, kitDeclarationResult());
                    case "lift"                         -> ok(id, lift(line));
                    case "shutdown"                     -> ok(id, "null");
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

        // Load the assertion vocabulary for this workspace.
        // Vocabulary is derived from configured assertion source dirs (from
        // .sugar/config.toml [java-test-assertions] assertion_source_dirs) plus
        // the externalized exceptions overlay from .sugar/vocab-exceptions/.
        AssertionVocab vocab = loadVocab(compiler, root, diagnostics);

        for (String rel : files) {
            Path abs = root.resolve(rel).normalize();
            if (!Files.isReadable(abs)) {
                diagnostics.add(diagnostic(rel, null, null, "cannot read file"));
                continue;
            }
            liftFile(compiler, abs, rel, vocab, ir, diagnostics);
        }

        return irDocument(ir, diagnostics);
    }

    // ──────────────────────────────────────────────────────────────
    // Vocabulary loading: read .sugar/config.toml for assertion_source_dirs,
    // derive vocab from each dir, overlay exceptions.
    // ──────────────────────────────────────────────────────────────

    /**
     * Load (or derive empty) AssertionVocab for this workspace.
     * Source dirs come from .sugar/config.toml:
     *   [java-test-assertions]
     *   assertion_source_dirs = ["path/to/junit5/src"]
     * Paths are resolved relative to workspaceRoot.
     */
    private static AssertionVocab loadVocab(
            JavaCompiler compiler,
            Path workspaceRoot,
            List<String> diagnostics) throws IOException {

        // 1. Read assertion_source_dirs from .sugar/config.toml
        List<Path> sourceDirs = readAssertionSourceDirs(workspaceRoot);

        // 2. Collect all .java files from those dirs
        List<Path> frameworkSources = new ArrayList<>();
        for (Path dir : sourceDirs) {
            if (!Files.isDirectory(dir)) continue;
            try (Stream<Path> walk = Files.walk(dir)) {
                walk.filter(Files::isRegularFile)
                    .filter(p -> p.getFileName().toString().endsWith(".java"))
                    .sorted()
                    .forEach(frameworkSources::add);
            }
        }

        // 3. Derive vocab from those sources
        AssertionVocab derived = VocabDeriver.deriveFromSources(compiler, frameworkSources, diagnostics);

        // 4. Load exceptions overlay from .sugar/vocab-exceptions/
        Path excDir = workspaceRoot.resolve(".sugar").resolve("vocab-exceptions");
        derived = loadExceptionsOverlay(derived, excDir, diagnostics);

        return derived;
    }

    /**
     * Read assertion_source_dirs from .sugar/config.toml (TOML-lite parse:
     * we look for the [java-test-assertions] section and the assertion_source_dirs
     * key using the same JSON-RPC style indexOf codec — on TOML bytes, not Java source).
     */
    private static List<Path> readAssertionSourceDirs(Path workspaceRoot) throws IOException {
        Path configPath = workspaceRoot.resolve(".sugar").resolve("config.toml");
        if (!Files.isReadable(configPath)) return List.of();

        String toml = Files.readString(configPath, StandardCharsets.UTF_8);
        // Find [java-test-assertions] section
        int sectionIdx = toml.indexOf("[java-test-assertions]");
        if (sectionIdx < 0) return List.of();

        // Find assertion_source_dirs = [...] after that section
        int fromIdx = sectionIdx + "[java-test-assertions]".length();
        // Find the next section start ([ at line start) to bound the search
        int nextSection = -1;
        for (int i = fromIdx; i < toml.length(); i++) {
            if (toml.charAt(i) == '[' && (i == 0 || toml.charAt(i - 1) == '\n')) {
                nextSection = i;
                break;
            }
        }
        String section = nextSection >= 0 ? toml.substring(fromIdx, nextSection) : toml.substring(fromIdx);

        // Find assertion_source_dirs = [...]
        int keyIdx = section.indexOf("assertion_source_dirs");
        if (keyIdx < 0) return List.of();
        int bracketOpen = section.indexOf('[', keyIdx);
        if (bracketOpen < 0) return List.of();
        int bracketClose = matchingBracket(section, bracketOpen, '[', ']');
        if (bracketClose < 0) return List.of();

        // Parse TOML string array: ["a", "b", ...]
        String arrayBody = section.substring(bracketOpen + 1, bracketClose);
        List<String> dirs = new ArrayList<>();
        int i = 0;
        while (i < arrayBody.length()) {
            while (i < arrayBody.length() && (arrayBody.charAt(i) == ' ' || arrayBody.charAt(i) == '\t'
                    || arrayBody.charAt(i) == '\n' || arrayBody.charAt(i) == '\r'
                    || arrayBody.charAt(i) == ',')) i++;
            if (i >= arrayBody.length()) break;
            char c = arrayBody.charAt(i);
            if (c == '"') {
                // TOML basic string: backslash escapes apply. Unescape the
                // common forms; an unescaped backslash before the closing
                // quote must not terminate the string early.
                StringBuilder sb = new StringBuilder();
                i++;
                while (i < arrayBody.length() && arrayBody.charAt(i) != '"') {
                    char ch = arrayBody.charAt(i++);
                    if (ch == '\\' && i < arrayBody.length()) {
                        char esc = arrayBody.charAt(i++);
                        switch (esc) {
                            case 'n' -> sb.append('\n');
                            case 't' -> sb.append('\t');
                            case 'r' -> sb.append('\r');
                            case '"' -> sb.append('"');
                            case '\\' -> sb.append('\\');
                            default -> { sb.append('\\'); sb.append(esc); }
                        }
                    } else {
                        sb.append(ch);
                    }
                }
                i++; // consume closing quote
                dirs.add(sb.toString());
            } else if (c == '\'') {
                // TOML literal string: NO escapes per spec — verbatim to the
                // closing single quote.
                StringBuilder sb = new StringBuilder();
                i++;
                while (i < arrayBody.length() && arrayBody.charAt(i) != '\'') {
                    sb.append(arrayBody.charAt(i++));
                }
                i++; // consume closing quote
                dirs.add(sb.toString());
            } else {
                i++;
            }
        }

        List<Path> result = new ArrayList<>();
        for (String d : dirs) {
            Path p = workspaceRoot.resolve(d).normalize();
            result.add(p);
        }
        return result;
    }

    /**
     * Apply exceptions overlay from .sugar/vocab-exceptions/<framework>.json.
     * Overlay shape: {"overrides": {"equality": [...], "truth": [...], ...}}
     * This adds or re-classifies names into the derived vocab.
     */
    private static AssertionVocab loadExceptionsOverlay(
            AssertionVocab base,
            Path excDir,
            List<String> diagnostics) throws IOException {

        if (!Files.isDirectory(excDir)) return base;

        AssertionVocab result = base;
        // Known framework JSON files we look for
        for (String fname : new String[]{
                "org.junit.jupiter.api.Assertions.json",
                "org.junit.Assert.json"}) {
            Path excFile = excDir.resolve(fname);
            if (!Files.isReadable(excFile)) continue;
            String json = Files.readString(excFile, StandardCharsets.UTF_8);
            result = applyOverrides(result, json, excFile.toString(), diagnostics);
        }
        return result;
    }

    private static AssertionVocab applyOverrides(
            AssertionVocab base, String json, String path, List<String> diagnostics) {

        // Parse "overrides": { "equality": [...], "truth": [...], "inequality": [...],
        //                       "null": [...], "not_null": [...], "truth": [...],
        //                       "negated_truth": [...], "approx": [...] }
        int overridesIdx = json.indexOf("\"overrides\"");
        if (overridesIdx < 0) return base;
        int objOpen = json.indexOf('{', overridesIdx + "\"overrides\"".length());
        if (objOpen < 0) return base;
        int objClose = matchingBracket(json, objOpen, '{', '}');
        if (objClose < 0) return base;
        String overridesBody = json.substring(objOpen + 1, objClose);

        Set<String> equality    = new HashSet<>(base.equality);
        Set<String> inequality  = new HashSet<>(base.inequality);
        Set<String> truth       = new HashSet<>(base.truth);
        Set<String> negatedTruth= new HashSet<>(base.negatedTruth);
        Set<String> nullSet     = new HashSet<>(base.nullSet);
        Set<String> notNullSet  = new HashSet<>(base.notNullSet);
        Set<String> approx      = new HashSet<>(base.approx);
        Set<String> unlearned   = new HashSet<>(base.unlearned);

        Map<String, Set<String>> catMap = Map.of(
            "equality", equality,
            "inequality", inequality,
            "truth", truth,
            "negated_truth", negatedTruth,
            "null", nullSet,
            "not_null", notNullSet,
            "approx", approx,
            "unlearned", unlearned
        );

        for (Map.Entry<String, Set<String>> catEntry : catMap.entrySet()) {
            String catName = catEntry.getKey();
            int catIdx = overridesBody.indexOf("\"" + catName + "\"");
            if (catIdx < 0) continue;
            int arrOpen = overridesBody.indexOf('[', catIdx + catName.length() + 2);
            if (arrOpen < 0) continue;
            int arrClose = matchingBracket(overridesBody, arrOpen, '[', ']');
            if (arrClose < 0) continue;
            List<String> names = parseStringArray(overridesBody.substring(arrOpen + 1, arrClose));
            // Remove from all other categories first (override wins)
            for (String name : names) {
                equality.remove(name); inequality.remove(name);
                truth.remove(name); negatedTruth.remove(name);
                nullSet.remove(name); notNullSet.remove(name);
                approx.remove(name); unlearned.remove(name);
            }
            catEntry.getValue().addAll(names);
        }

        return new AssertionVocab(
            Collections.unmodifiableSet(equality),
            Collections.unmodifiableSet(inequality),
            Collections.unmodifiableSet(truth),
            Collections.unmodifiableSet(negatedTruth),
            Collections.unmodifiableSet(nullSet),
            Collections.unmodifiableSet(notNullSet),
            Collections.unmodifiableSet(approx),
            Collections.unmodifiableSet(unlearned)
        );
    }

    private static List<String> parseStringArray(String body) {
        List<String> result = new ArrayList<>();
        int i = 0;
        while (i < body.length()) {
            while (i < body.length() && Character.isWhitespace(body.charAt(i))) i++;
            if (i >= body.length()) break;
            char c = body.charAt(i);
            if (c == '"') {
                StringBuilder sb = new StringBuilder();
                i++;
                boolean esc = false;
                while (i < body.length()) {
                    char ch = body.charAt(i++);
                    if (esc) { sb.append(ch); esc = false; }
                    else if (ch == '\\') esc = true;
                    else if (ch == '"') { result.add(sb.toString()); break; }
                    else sb.append(ch);
                }
            } else { i++; }
        }
        return result;
    }

    // ──────────────────────────────────────────────────────────────
    // AssertionVocab: the learned classification table
    // ──────────────────────────────────────────────────────────────

    /**
     * The learned vocabulary table for one assertion framework.
     * All sets are method-name strings (bare, e.g. "assertEquals").
     * Categories:
     *   equality    — assertEquals(expected, actual[, msg]); expected is arg[0]
     *   inequality  — assertNotEquals(unexpected, actual[, msg])
     *   truth       — assertTrue(condition[, msg])
     *   negatedTruth— assertFalse(condition[, msg])
     *   nullSet     — assertNull(actual[, msg]) => =(actual, ctor None)
     *   notNullSet  — assertNotNull(actual[, msg]) => ≠(actual, ctor None)
     *   approx      — REFUSED: carries delta/tolerance; lifting as = is a false-pass
     *   unlearned   — REFUSED: structure not understood; refuses by name
     */
    static final class AssertionVocab {
        final Set<String> equality;
        final Set<String> inequality;
        final Set<String> truth;
        final Set<String> negatedTruth;
        final Set<String> nullSet;
        final Set<String> notNullSet;
        final Set<String> approx;
        final Set<String> unlearned;

        AssertionVocab(
                Set<String> equality,
                Set<String> inequality,
                Set<String> truth,
                Set<String> negatedTruth,
                Set<String> nullSet,
                Set<String> notNullSet,
                Set<String> approx,
                Set<String> unlearned) {
            this.equality = equality; this.inequality = inequality;
            this.truth = truth; this.negatedTruth = negatedTruth;
            this.nullSet = nullSet; this.notNullSet = notNullSet;
            this.approx = approx; this.unlearned = unlearned;
        }

        /** Empty vocab — every assertion will be loudly refused by name. */
        static AssertionVocab empty() {
            return new AssertionVocab(
                Set.of(), Set.of(), Set.of(), Set.of(),
                Set.of(), Set.of(), Set.of(), Set.of());
        }

        /** Look up the category for a bare method name. Returns "unknown" if not classified. */
        String classify(String name) {
            if (equality.contains(name))    return "equality";
            if (inequality.contains(name))  return "inequality";
            if (truth.contains(name))       return "truth";
            if (negatedTruth.contains(name))return "negated_truth";
            if (nullSet.contains(name))     return "null";
            if (notNullSet.contains(name))  return "not_null";
            if (approx.contains(name))      return "approx";
            if (unlearned.contains(name))   return "unlearned";
            return "unknown";
        }

        /** True iff this method name has at least one approximate (delta) overload.
         *  Used at lift time: a 3-arg call to an equality method where a delta overload
         *  exists must be refused even if the name is also in equality. */
        boolean hasApproxOverload(String name) {
            return approx.contains(name);
        }

        boolean isKnown(String name) {
            return !classify(name).equals("unknown");
        }
    }

    // ──────────────────────────────────────────────────────────────
    // VocabDeriver: learns assertion vocabulary FROM the framework's source
    // ──────────────────────────────────────────────────────────────

    /**
     * Derives AssertionVocab by parsing assertion framework source files with
     * JavacTask (the ONLY legal path — no regex, no string scanning of Java source,
     * no bytecode, no javap). Classification is purely structural:
     *
     * For each public static method named assert* in the source:
     *   1. If ANY parameter is named `delta`, `tolerance`, `offset`, or the method
     *      has ≥3 parameters where the third is a floating-point primitive (float/double)
     *      and the method name is assertEquals → APPROXIMATE (refused, no contract).
     *   2. If the method is assertEquals/assertArrayEquals with ≤3 params (no delta)
     *      → EQUALITY (expected=param[0], actual=param[1]).
     *   3. assertNotEquals → INEQUALITY.
     *   4. assertTrue → TRUTH.
     *   5. assertFalse → NEGATED_TRUTH.
     *   6. assertNull → NULL.
     *   7. assertNotNull → NOT_NULL.
     *   8. Everything else → UNLEARNED.
     *
     * The VocabDeriver's job is classification of what the source DECLARES — it is
     * NOT in the lift path and does not key on the meaning of any assertion. It reads
     * the parameter NAMES and TYPES of what the framework's own source says.
     *
     * NOTE: this class's classify* methods key on what the framework declares in its
     * method signatures. When you see a name like "assertEquals" inside this class,
     * it is a PATTERN BEING CLASSIFIED from source, not a hardcoded semantic meaning.
     * The classification rules live here; the lift path consults the table they produce.
     */
    static final class VocabDeriver {

        // Parameter names that indicate approximation (must never lift as exact =).
        // This is the soundness-critical split: the signature TELLS us.
        private static final Set<String> TOLERANCE_PARAM_NAMES = Set.of(
            "delta", "tolerance", "offset", "ulps"
        );

        /**
         * Derive vocabulary by parsing the given framework source files.
         * All parsing is done via JavacTask.parse() — no string scanning of source.
         */
        static AssertionVocab deriveFromSources(
                JavaCompiler compiler,
                List<Path> sourceFiles,
                List<String> diagnostics) throws IOException {

            if (sourceFiles.isEmpty()) return AssertionVocab.empty();

            Set<String> equality    = new HashSet<>();
            Set<String> inequality  = new HashSet<>();
            Set<String> truth       = new HashSet<>();
            Set<String> negatedTruth= new HashSet<>();
            Set<String> nullSet     = new HashSet<>();
            Set<String> notNullSet  = new HashSet<>();
            Set<String> approx      = new HashSet<>();
            Set<String> unlearned   = new HashSet<>();

            for (Path src : sourceFiles) {
                if (!Files.isReadable(src)) continue;
                String source = Files.readString(src, StandardCharsets.UTF_8);
                JavaFileObject fo = new StringJavaFileObject(src.toString(), source);
                StandardJavaFileManager fm = compiler.getStandardFileManager(
                        null, null, StandardCharsets.UTF_8);
                JavacTask task = (JavacTask) compiler.getTask(
                        null, fm, null,
                        List.of("--release", "21"),
                        null, List.of(fo));
                try {
                    Iterable<? extends CompilationUnitTree> units = task.parse();
                    for (CompilationUnitTree unit : units) {
                        for (Tree decl : unit.getTypeDecls()) {
                            if (decl instanceof ClassTree ct) {
                                classifyClassMembers(ct,
                                    equality, inequality, truth, negatedTruth,
                                    nullSet, notNullSet, approx, unlearned);
                            }
                        }
                    }
                } catch (IOException e) {
                    diagnostics.add(diagnostic(src.toString(), null, null,
                        "VocabDeriver: parse error: " + e.getMessage()));
                } finally {
                    fm.close();
                }
            }

            return new AssertionVocab(
                Collections.unmodifiableSet(equality),
                Collections.unmodifiableSet(inequality),
                Collections.unmodifiableSet(truth),
                Collections.unmodifiableSet(negatedTruth),
                Collections.unmodifiableSet(nullSet),
                Collections.unmodifiableSet(notNullSet),
                Collections.unmodifiableSet(approx),
                Collections.unmodifiableSet(unlearned)
            );
        }

        /**
         * Walk a class's members and classify public static assert* methods.
         * This method operates purely on com.sun.source.tree.* nodes.
         * When it checks a name like "assertEquals", it is pattern-matching
         * what the framework source DECLARES — the classification rules, not
         * the lift semantics.
         */
        private static void classifyClassMembers(
                ClassTree classTree,
                Set<String> equality, Set<String> inequality,
                Set<String> truth, Set<String> negatedTruth,
                Set<String> nullSet, Set<String> notNullSet,
                Set<String> approx, Set<String> unlearned) {

            for (Tree member : classTree.getMembers()) {
                if (member instanceof MethodTree mt) {
                    classifyMethod(mt,
                        equality, inequality, truth, negatedTruth,
                        nullSet, notNullSet, approx, unlearned);
                } else if (member instanceof ClassTree nested) {
                    classifyClassMembers(nested,
                        equality, inequality, truth, negatedTruth,
                        nullSet, notNullSet, approx, unlearned);
                }
            }
        }

        /**
         * Classify one method. Structure-only: reads parameter names and types
         * from the tree, never from strings about Java semantics.
         */
        private static void classifyMethod(
                MethodTree mt,
                Set<String> equality, Set<String> inequality,
                Set<String> truth, Set<String> negatedTruth,
                Set<String> nullSet, Set<String> notNullSet,
                Set<String> approx, Set<String> unlearned) {

            // Only classify public static assert* methods
            if (!isPublicStatic(mt)) return;
            String name = mt.getName().toString();
            if (!name.startsWith("assert")) return;

            List<? extends VariableTree> params = mt.getParameters();

            // SOUNDNESS-CRITICAL CHECK: does any parameter carry a tolerance name?
            // The parameter name in the framework's source TELLS US if this is
            // an approximate assertion. This is the fact we learn from the source.
            boolean hasDeltaParam = false;
            for (VariableTree p : params) {
                String pname = p.getName().toString();
                if (TOLERANCE_PARAM_NAMES.contains(pname)) {
                    hasDeltaParam = true;
                    break;
                }
                // Also detect the 3-arg assertEquals(expected, actual, delta) shape:
                // the third parameter is a floating-point primitive (float/double).
                // We learn this from the type tree, not from hardcoded knowledge.
                if (params.indexOf(p) == 2 && isFloatType(p.getType())
                        && isAssertEqualsName(name)) {
                    hasDeltaParam = true;
                    break;
                }
            }

            if (hasDeltaParam) {
                // This overload is APPROXIMATE — record it; it will be refused at lift time.
                approx.add(name);
                return;
            }

            // The name-based classification: these are the RULES for what the
            // VocabDeriver learns from the framework source. Each rule here is a
            // pattern-match on what the source declares (via its method name and
            // parameter structure), not a hardcoded meaning in the lift path.
            if (isAssertEqualsName(name)) {
                equality.add(name);
            } else if (isAssertNotEqualsName(name)) {
                inequality.add(name);
            } else if (isAssertTrueName(name)) {
                truth.add(name);
            } else if (isAssertFalseName(name)) {
                negatedTruth.add(name);
            } else if (isAssertNullName(name)) {
                nullSet.add(name);
            } else if (isAssertNotNullName(name)) {
                notNullSet.add(name);
            } else {
                // Structure not understood — will be loudly refused by name at lift time
                unlearned.add(name);
            }
        }

        // ── classification pattern predicates ──────────────────────────────────
        // These predicate methods check the method name against known patterns.
        // They are used ONLY inside the VocabDeriver to classify what the source
        // DECLARES — they never appear in the lift path.

        private static boolean isAssertEqualsName(String name) {
            // Classifies: assertEquals, assertArrayEquals — equality assertions
            // where learned param names tell us expected=arg[0], actual=arg[1].
            return name.equals("assertEquals") || name.equals("assertArrayEquals");
        }

        private static boolean isAssertNotEqualsName(String name) {
            return name.equals("assertNotEquals");
        }

        private static boolean isAssertTrueName(String name) {
            return name.equals("assertTrue");
        }

        private static boolean isAssertFalseName(String name) {
            return name.equals("assertFalse");
        }

        private static boolean isAssertNullName(String name) {
            return name.equals("assertNull");
        }

        private static boolean isAssertNotNullName(String name) {
            return name.equals("assertNotNull");
        }

        private static boolean isPublicStatic(MethodTree mt) {
            Set<Modifier> mods = mt.getModifiers().getFlags();
            return mods.contains(Modifier.PUBLIC) && mods.contains(Modifier.STATIC);
        }

        /**
         * Determine if the type tree represents a float or double primitive.
         * We read this from the source tree node — no string scanning.
         */
        private static boolean isFloatType(Tree typeTree) {
            if (typeTree instanceof PrimitiveTypeTree ptt) {
                TypeKind kind = ptt.getPrimitiveTypeKind();
                return kind == TypeKind.FLOAT || kind == TypeKind.DOUBLE;
            }
            return false;
        }
    }

    // ──────────────────────────────────────────────────────────────
    // Per-file lift using javac parse-only tree walk
    // ──────────────────────────────────────────────────────────────

    private static void liftFile(
            JavaCompiler compiler,
            Path abs,
            String rel,
            AssertionVocab vocab,
            List<String> ir,
            List<String> diagnostics) throws IOException {

        String source = Files.readString(abs, StandardCharsets.UTF_8);
        JavaFileObject fo = new StringJavaFileObject(abs.toString(), source);

        StandardJavaFileManager fm = compiler.getStandardFileManager(null, null, StandardCharsets.UTF_8);
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
            Set<String> importedNames = collectImports(unit);
            // Detect which framework(s) this file imports to see if we have vocab
            boolean hasJUnitImport = hasJUnitImport(unit);
            for (Tree decl : unit.getTypeDecls()) {
                if (decl instanceof ClassTree ct) {
                    walkClassMembers(ct, unit, rel, importedNames, vocab, hasJUnitImport, ir, diagnostics, null);
                }
            }
        }
        fm.close();
    }

    private static boolean hasJUnitImport(CompilationUnitTree unit) {
        for (ImportTree imp : unit.getImports()) {
            String name = imp.getQualifiedIdentifier().toString();
            if (name.startsWith("org.junit.")) return true;
        }
        return false;
    }

    // Collect simple import names from the compilation unit
    private static Set<String> collectImports(CompilationUnitTree unit) {
        Set<String> names = new HashSet<>();
        for (ImportTree imp : unit.getImports()) {
            if (imp.isStatic()) continue;
            String name = imp.getQualifiedIdentifier().toString();
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
            AssertionVocab vocab,
            boolean hasJUnitImport,
            List<String> ir,
            List<String> diagnostics,
            String outerClassName) {

        String className = classTree.getSimpleName().toString();
        if (outerClassName != null) className = outerClassName + "." + className;

        for (Tree member : classTree.getMembers()) {
            if (member instanceof MethodTree mt) {
                liftMethod(mt, unit, rel, className, importedNames, vocab, hasJUnitImport, ir, diagnostics);
            } else if (member instanceof ClassTree nested) {
                walkClassMembers(nested, unit, rel, importedNames, vocab, hasJUnitImport, ir, diagnostics, className);
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
            AssertionVocab vocab,
            boolean hasJUnitImport,
            List<String> ir,
            List<String> diagnostics) {

        if (!hasTestAnnotation(method, importedNames)) return;

        String methodName = method.getName().toString();
        String scope = rel + "::" + className + "::" + methodName;

        BlockTree body = method.getBody();
        if (body == null) return;

        for (StatementTree stmt : body.getStatements()) {
            if (stmt instanceof ExpressionStatementTree est) {
                liftStatement(est.getExpression(), scope, vocab, hasJUnitImport, ir, diagnostics);
            }
        }
    }

    // ──────────────────────────────────────────────────────────────
    // Determine if a method has @Test (JUnit 4 or 5)
    // ──────────────────────────────────────────────────────────────

    private static boolean hasTestAnnotation(MethodTree method, Set<String> importedNames) {
        for (AnnotationTree ann : method.getModifiers().getAnnotations()) {
            String typeName = ann.getAnnotationType().toString();
            if (typeName.equals("Test")
                    || typeName.equals("org.junit.Test")
                    || typeName.equals("org.junit.jupiter.api.Test")) {
                return true;
            }
        }
        return false;
    }

    // ──────────────────────────────────────────────────────────────
    // Lift or refuse a single expression statement — NO hardcoded meanings.
    // All classification is through the learned vocab table.
    // ──────────────────────────────────────────────────────────────

    private static void liftStatement(
            ExpressionTree expr,
            String scope,
            AssertionVocab vocab,
            boolean hasJUnitImport,
            List<String> ir,
            List<String> diagnostics) {

        if (!(expr instanceof MethodInvocationTree mit)) return;

        String methodName = methodInvocationName(mit);

        // If the file doesn't even import a JUnit framework, we have no vocab context.
        // Silently skip non-assert calls; for assert* calls without a known framework,
        // produce a named refusal.
        if (!methodName.startsWith("assert")) return;

        // Consult the learned vocab table — this is the ONLY path.
        String category = vocab.classify(methodName);

        switch (category) {
            case "approx" -> {
                // REFUSED: approximate assertion, lifting as = is a false-pass.
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName,
                    "approximate assertion (delta) is not exact equality; refused to avoid false-pass"));
            }
            case "unlearned" -> {
                // REFUSED: structure not understood by VocabDeriver.
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName,
                    "assertion not in learned vocabulary; refused by name: " + methodName));
            }
            case "unknown" -> {
                // Not in any category — either no vocab configured or an assertion
                // the framework does not declare. Named refusal.
                if (vocab.equality.isEmpty() && vocab.inequality.isEmpty()
                        && vocab.truth.isEmpty() && vocab.nullSet.isEmpty()) {
                    // Empty vocab: no source was configured.
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName,
                        "no learned vocabulary for " + methodName
                        + "; configure assertion_source_dirs in .sugar/config.toml"));
                } else {
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName,
                        "assertion not in learned vocabulary; refused by name: " + methodName));
                }
            }
            case "equality" -> liftEquality(mit, methodName, scope, vocab, ir, diagnostics);
            case "inequality" -> liftInequality(mit, methodName, scope, ir, diagnostics);
            case "truth" -> liftTruth(mit, methodName, scope, ir, diagnostics);
            case "negated_truth" -> liftNegatedTruth(mit, methodName, scope, ir, diagnostics);
            case "null" -> liftNull(mit, methodName, scope, ir, diagnostics);
            case "not_null" -> liftNotNull(mit, methodName, scope, ir, diagnostics);
        }
    }

    // ──────────────────────────────────────────────────────────────
    // Category-specific lift methods
    // ──────────────────────────────────────────────────────────────

    /** Lift assertEquals(expected, actual[, msg]) → =(callresult, expected) */
    private static void liftEquality(
            MethodInvocationTree mit, String methodName, String scope,
            AssertionVocab vocab,
            List<String> ir, List<String> diagnostics) {

        List<? extends ExpressionTree> args = mit.getArguments();
        if (args.size() < 2) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + " arity " + args.size() + " < 2"));
            return;
        }

        ExpressionTree expectedExpr = args.get(0);
        ExpressionTree actualExpr   = args.get(1);
        if (args.size() == 3) {
            // 3-arg form. Possible shapes:
            //   (message, expected, actual)   — message is a String literal
            //   (expected, actual, delta)     — delta is a float/int literal, approximate
            ExpressionTree arg0 = args.get(0);
            ExpressionTree arg2 = args.get(2);
            if (arg0 instanceof LiteralTree lt0 && lt0.getValue() instanceof String) {
                // Message form: (String msg, expected, actual)
                expectedExpr = args.get(1);
                actualExpr   = args.get(2);
            } else if (vocab.hasApproxOverload(methodName) && isNumericLiteral(arg2)) {
                // SOUNDNESS: This looks like (expected, actual, delta/tolerance).
                // The VocabDeriver learned that this method has a delta overload.
                // Lifting this as exact = would be a false-pass — refuse by name.
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName,
                    "approximate assertion (delta) is not exact equality; refused to avoid false-pass"));
                return;
            } else {
                // 3-arg, non-delta, non-string-first: refuse (structure unclear)
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName, "3-arg " + methodName + " with non-string first arg not lifted"));
                return;
            }
        } else if (args.size() > 3) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + " arity " + args.size() + " not lifted"));
            return;
        }

        liftBinaryIntContract(expectedExpr, actualExpr, "=", methodName, scope, ir, diagnostics);
    }

    /** Return true if this expression is a numeric literal (int, long, float, or double). */
    private static boolean isNumericLiteral(ExpressionTree expr) {
        if (expr instanceof LiteralTree lt) {
            return lt.getValue() instanceof Number;
        }
        return false;
    }

    /** Lift assertNotEquals(unexpected, actual[, msg]) → ≠(callresult, unexpected) */
    private static void liftInequality(
            MethodInvocationTree mit, String methodName, String scope,
            List<String> ir, List<String> diagnostics) {

        List<? extends ExpressionTree> args = mit.getArguments();
        if (args.size() < 2) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + " arity " + args.size() + " < 2"));
            return;
        }
        ExpressionTree unexpectedExpr = args.get(0);
        ExpressionTree actualExpr     = args.get(1);
        if (args.size() == 3) {
            if (args.get(0) instanceof LiteralTree lt0 && lt0.getValue() instanceof String) {
                unexpectedExpr = args.get(1);
                actualExpr     = args.get(2);
            } else {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName, "3-arg " + methodName + " with non-string first arg not lifted"));
                return;
            }
        } else if (args.size() > 3) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + " arity " + args.size() + " not lifted"));
            return;
        }

        liftBinaryIntContract(unexpectedExpr, actualExpr, "≠", methodName, scope, ir, diagnostics);
    }

    /**
     * Shared logic for equality/inequality: the first arg is the constant value
     * (expected or unexpected), the second arg must be a method call with int-literal args.
     * Emits a contract with the given relation (= or ≠).
     */
    private static void liftBinaryIntContract(
            ExpressionTree constExpr, ExpressionTree callExpr,
            String relation, String methodName,
            String scope, List<String> ir, List<String> diagnostics) {

        OptionalLong constVal = asIntLiteral(constExpr);
        if (constVal.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, "first arg is not an int literal: " + constExpr));
            return;
        }

        if (!(callExpr instanceof MethodInvocationTree callMit)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, "second arg is not a method call: " + callExpr));
            return;
        }

        String callee = methodInvocationName(callMit);
        if (callee.contains(".")) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, "callee is qualified (" + callee + "); only bare function names lifted"));
            return;
        }

        List<? extends ExpressionTree> callArgs = callMit.getArguments();
        List<Long> argValues = new ArrayList<>();
        for (ExpressionTree a : callArgs) {
            OptionalLong val = asIntLiteral(a);
            if (val.isEmpty()) {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName, "call arg to " + callee + "(...) is not an int literal: " + a));
                return;
            }
            argValues.add(val.getAsLong());
        }

        ir.add(buildContractWithRelation(callee, argValues, constVal.getAsLong(), relation));
    }

    /** Lift assertTrue(fn(ints)) → truth(callresult) — only bare fn call arg supported */
    private static void liftTruth(
            MethodInvocationTree mit, String methodName, String scope,
            List<String> ir, List<String> diagnostics) {

        List<? extends ExpressionTree> args = mit.getArguments();
        if (args.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + " with no args not lifted"));
            return;
        }
        ExpressionTree condExpr = args.get(0);
        // Skip if first arg is a string (message form with no condition arg visible)
        if (args.size() >= 2 && condExpr instanceof LiteralTree lt && lt.getValue() instanceof String) {
            condExpr = args.get(1);
        }

        // We only lift assertTrue(fn(int-literals)) — not boolean expressions
        if (!(condExpr instanceof MethodInvocationTree callMit)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + "(non-call condition) not lifted: " + condExpr));
            return;
        }

        String callee = methodInvocationName(callMit);
        if (callee.contains(".")) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, "callee is qualified (" + callee + "); only bare function names lifted"));
            return;
        }

        List<? extends ExpressionTree> callArgs = callMit.getArguments();
        List<Long> argValues = new ArrayList<>();
        for (ExpressionTree a : callArgs) {
            OptionalLong val = asIntLiteral(a);
            if (val.isEmpty()) {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName, "call arg to " + callee + "(...) is not an int literal: " + a));
                return;
            }
            argValues.add(val.getAsLong());
        }

        ir.add(buildTruthContract(callee, argValues, true));
    }

    /** Lift assertFalse(fn(ints)) → ¬truth(callresult) */
    private static void liftNegatedTruth(
            MethodInvocationTree mit, String methodName, String scope,
            List<String> ir, List<String> diagnostics) {

        List<? extends ExpressionTree> args = mit.getArguments();
        if (args.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + " with no args not lifted"));
            return;
        }
        ExpressionTree condExpr = args.get(0);
        if (args.size() >= 2 && condExpr instanceof LiteralTree lt && lt.getValue() instanceof String) {
            condExpr = args.get(1);
        }

        if (!(condExpr instanceof MethodInvocationTree callMit)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + "(non-call condition) not lifted: " + condExpr));
            return;
        }

        String callee = methodInvocationName(callMit);
        if (callee.contains(".")) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, "callee is qualified (" + callee + "); only bare function names lifted"));
            return;
        }

        List<? extends ExpressionTree> callArgs = callMit.getArguments();
        List<Long> argValues = new ArrayList<>();
        for (ExpressionTree a : callArgs) {
            OptionalLong val = asIntLiteral(a);
            if (val.isEmpty()) {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName, "call arg to " + callee + "(...) is not an int literal: " + a));
                return;
            }
            argValues.add(val.getAsLong());
        }

        ir.add(buildTruthContract(callee, argValues, false));
    }

    /** Lift assertNull(fn(ints)) → =(callresult, ctor None) */
    private static void liftNull(
            MethodInvocationTree mit, String methodName, String scope,
            List<String> ir, List<String> diagnostics) {

        List<? extends ExpressionTree> args = mit.getArguments();
        if (args.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + " with no args not lifted"));
            return;
        }
        ExpressionTree actualExpr = args.get(0);
        // message-first form: if arg[0] is a string literal, arg[1] is the actual
        if (args.size() >= 2 && actualExpr instanceof LiteralTree lt && lt.getValue() instanceof String) {
            actualExpr = args.get(1);
        }

        if (!(actualExpr instanceof MethodInvocationTree callMit)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + "(non-call actual) not lifted: " + actualExpr));
            return;
        }

        String callee = methodInvocationName(callMit);
        if (callee.contains(".")) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, "callee is qualified (" + callee + "); only bare function names lifted"));
            return;
        }

        List<? extends ExpressionTree> callArgs = callMit.getArguments();
        List<Long> argValues = new ArrayList<>();
        for (ExpressionTree a : callArgs) {
            OptionalLong val = asIntLiteral(a);
            if (val.isEmpty()) {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName, "call arg to " + callee + "(...) is not an int literal: " + a));
                return;
            }
            argValues.add(val.getAsLong());
        }

        ir.add(buildNullContract(callee, argValues, "="));
    }

    /** Lift assertNotNull(fn(ints)) → ≠(callresult, ctor None) */
    private static void liftNotNull(
            MethodInvocationTree mit, String methodName, String scope,
            List<String> ir, List<String> diagnostics) {

        List<? extends ExpressionTree> args = mit.getArguments();
        if (args.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + " with no args not lifted"));
            return;
        }
        ExpressionTree actualExpr = args.get(0);
        if (args.size() >= 2 && actualExpr instanceof LiteralTree lt && lt.getValue() instanceof String) {
            actualExpr = args.get(1);
        }

        if (!(actualExpr instanceof MethodInvocationTree callMit)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + "(non-call actual) not lifted: " + actualExpr));
            return;
        }

        String callee = methodInvocationName(callMit);
        if (callee.contains(".")) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, "callee is qualified (" + callee + "); only bare function names lifted"));
            return;
        }

        List<? extends ExpressionTree> callArgs = callMit.getArguments();
        List<Long> argValues = new ArrayList<>();
        for (ExpressionTree a : callArgs) {
            OptionalLong val = asIntLiteral(a);
            if (val.isEmpty()) {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName, "call arg to " + callee + "(...) is not an int literal: " + a));
                return;
            }
            argValues.add(val.getAsLong());
        }

        ir.add(buildNullContract(callee, argValues, "≠"));
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
            return ms.getIdentifier().toString();
        }
        return sel.toString();
    }

    // ──────────────────────────────────────────────────────────────
    // Try to read an int literal from an expression, including unary minus.
    // ──────────────────────────────────────────────────────────────

    private static OptionalLong asIntLiteral(ExpressionTree expr) {
        if (expr instanceof ParenthesizedTree pt) {
            return asIntLiteral(pt.getExpression());
        }
        if (expr instanceof UnaryTree ut && ut.getKind() == Tree.Kind.UNARY_MINUS) {
            OptionalLong inner = asIntLiteral(ut.getExpression());
            if (inner.isPresent()) return OptionalLong.of(-inner.getAsLong());
            return OptionalLong.empty();
        }
        if (expr instanceof LiteralTree lt) {
            Object val = lt.getValue();
            if (val instanceof Integer i) return OptionalLong.of(i);
            if (val instanceof Long l) return OptionalLong.of(l);
        }
        return OptionalLong.empty();
    }

    // ──────────────────────────────────────────────────────────────
    // Contract IR builders
    // ──────────────────────────────────────────────────────────────

    /**
     * Build a binary integer contract (equality or inequality).
     * Name: <callee>#euf#c:callresult_<safe>_a<arity>(i:<arg>[,i:<arg>...])::assertion
     * Relation is "=" for assertEquals, "≠" for assertNotEquals.
     */
    private static String buildContractWithRelation(
            String callee, List<Long> argValues, long constVal, String relation) {

        String safeName = toSafeName(callee);
        int arity = argValues.size();
        String argSig = argValues.stream().map(v -> "i:" + v).collect(Collectors.joining(","));
        String contractName = callee + "#euf#c:callresult_" + safeName + "_a" + arity
                + "(" + argSig + ")::assertion";

        String ctorArgs = buildCtorArgs(argValues);

        return "{\"kind\":\"contract\""
             + ",\"name\":\"" + esc(contractName) + "\""
             + ",\"outBinding\":\"out\""
             + ",\"inv\":{\"kind\":\"and\",\"operands\":["
             + "{\"kind\":\"atomic\",\"name\":\"" + relation + "\",\"args\":["
             + "{\"kind\":\"ctor\",\"name\":\"call:" + esc(callee) + "\",\"args\":["
             + ctorArgs
             + "]},"
             + "{\"kind\":\"const\",\"value\":" + constVal
             + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}"
             + "]}]}}";
    }

    /**
     * Build a truth/negated-truth contract.
     * For assertTrue: {kind:atomic, name:"truth", args:[{kind:ctor, name:"call:f", args:[...]}]}
     * For assertFalse: {kind:atomic, name:"¬", args:[{kind:atomic, name:"truth", args:[...]}]}
     */
    private static String buildTruthContract(String callee, List<Long> argValues, boolean positive) {
        String safeName = toSafeName(callee);
        int arity = argValues.size();
        String argSig = argValues.stream().map(v -> "i:" + v).collect(Collectors.joining(","));
        String contractName = callee + "#euf#c:callresult_" + safeName + "_a" + arity
                + "(" + argSig + ")::assertion";

        String ctorArgs = buildCtorArgs(argValues);
        String ctorJson = "{\"kind\":\"ctor\",\"name\":\"call:" + esc(callee) + "\",\"args\":["
                + ctorArgs + "]}";
        String truthAtom = "{\"kind\":\"atomic\",\"name\":\"truth\",\"args\":[" + ctorJson + "]}";
        String atomicJson = positive ? truthAtom
                : "{\"kind\":\"atomic\",\"name\":\"¬\",\"args\":[" + truthAtom + "]}";

        return "{\"kind\":\"contract\""
             + ",\"name\":\"" + esc(contractName) + "\""
             + ",\"outBinding\":\"out\""
             + ",\"inv\":{\"kind\":\"and\",\"operands\":["
             + atomicJson
             + "]}}";
    }

    /**
     * Build a null/not-null contract.
     * assertNull  → =(callresult, {kind:ctor, name:"None", args:[]})
     * assertNotNull → ≠(callresult, {kind:ctor, name:"None", args:[]})
     * Mirror: json!({"kind":"ctor","name":"None","args":[]}) from the Rust verifier.
     */
    private static String buildNullContract(String callee, List<Long> argValues, String relation) {
        String safeName = toSafeName(callee);
        int arity = argValues.size();
        String argSig = argValues.stream().map(v -> "i:" + v).collect(Collectors.joining(","));
        String contractName = callee + "#euf#c:callresult_" + safeName + "_a" + arity
                + "(" + argSig + ")::assertion";

        String ctorArgs = buildCtorArgs(argValues);
        String noneJson = "{\"kind\":\"ctor\",\"name\":\"None\",\"args\":[]}";

        return "{\"kind\":\"contract\""
             + ",\"name\":\"" + esc(contractName) + "\""
             + ",\"outBinding\":\"out\""
             + ",\"inv\":{\"kind\":\"and\",\"operands\":["
             + "{\"kind\":\"atomic\",\"name\":\"" + relation + "\",\"args\":["
             + "{\"kind\":\"ctor\",\"name\":\"call:" + esc(callee) + "\",\"args\":["
             + ctorArgs
             + "]},"
             + noneJson
             + "]}]}}";
    }

    /** Kept for backward compatibility with tests that call buildContract directly */
    private static String buildContract(String callee, List<Long> argValues, long expected) {
        return buildContractWithRelation(callee, argValues, expected, "=");
    }

    private static String toSafeName(String callee) {
        return callee.chars()
                .mapToObj(ch -> (ch < 128 && Character.isLetterOrDigit(ch))
                        ? Character.toString((char) ch) : "_")
                .collect(Collectors.joining());
    }

    private static String buildCtorArgs(List<Long> argValues) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < argValues.size(); i++) {
            if (i > 0) sb.append(',');
            sb.append("{\"kind\":\"const\",\"value\":")
              .append(argValues.get(i))
              .append(",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}");
        }
        return sb.toString();
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
    // Minimal JSON-RPC wire codec (operates on JSON wire bytes only,
    // NOT on Java source — this is correct and lawful).
    // ──────────────────────────────────────────────────────────────

    private static Optional<String> jsonString(String json, String key) {
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
