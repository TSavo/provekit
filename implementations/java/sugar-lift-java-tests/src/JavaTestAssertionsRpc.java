// SPDX-License-Identifier: Apache-2.0
//
// Java-native JUnit/TestNG assertion lifter for the Sugar/ProvekIt substrate.
// Phase 4: TestNG support — proof that assertion vocabulary must be learned
// per-framework, never hardcoded.
//
// THE LAW: every fact about Java source comes from a com.sun.source.tree.*
// node. No regex, indexOf, split, or any string-scanning of Java source code
// is used here. JSON-RPC wire protocol codec uses indexOf/split on JSON bytes
// only -- not on Java source.
//
// THE POINT OF PHASE 4:
// TestNG's assertEquals(actual, expected) places ACTUAL FIRST — the reverse
// of JUnit's assertEquals(expected, actual). Hardcode either order and the
// other framework lifts every assertion backwards. The VocabDeriver reads
// the parameter NAMES from the vendored framework source to learn which
// argument carries the expected (constant) value: param[0]=="expected" →
// JUnit order (expectedArgIndex=0); param[0]=="actual" → TestNG order
// (expectedArgIndex=1). The lift path then uses this learned index — no
// framework-specific code in the lift path itself.
//
// Framework detection: org.junit.* imports → JUnit vocab; org.testng.* →
// TestNG vocab. File importing BOTH Assert classes → named refusal on
// every assertion (ambiguous vocabulary).
//
// assertThat (Hamcrest/AssertJ): not in any vendored source → unlearned →
// named refusal.
//
// The VocabDeriver reads each public static assert* method from the framework's
// source (e.g. org.junit.jupiter.api.Assertions), classifies it by structure:
//   - signature carries a `delta` / `tolerance` parameter  -> APPROXIMATE (refused)
//   - assertEquals(expected, actual[, msg])                -> EQUALITY, expectedArgIndex=0
//   - assertEquals(actual, expected[, msg]) [TestNG]       -> EQUALITY, expectedArgIndex=1
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
    private static final String VERSION = "0.4.0";

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

        // Load assertion vocabularies per framework for this workspace.
        // Each source dir in assertion_source_dirs is parsed separately;
        // the resulting vocab is stored in a MultiFrameworkVocab keyed by
        // the detected framework package (e.g. "org.junit", "org.testng").
        MultiFrameworkVocab multiVocab = loadMultiVocab(compiler, root, diagnostics);

        for (String rel : files) {
            Path abs = root.resolve(rel).normalize();
            if (!Files.isReadable(abs)) {
                diagnostics.add(diagnostic(rel, null, null, "cannot read file"));
                continue;
            }
            liftFile(compiler, abs, rel, multiVocab, ir, diagnostics);
        }

        return irDocument(ir, diagnostics);
    }

    // ──────────────────────────────────────────────────────────────
    // MultiFrameworkVocab: holds one AssertionVocab per framework
    // ──────────────────────────────────────────────────────────────

    /**
     * Holds the assertion vocabulary keyed by framework package prefix.
     * Keys are framework-package strings: "org.junit", "org.testng".
     * A key is present only if source was configured for that framework.
     */
    static final class MultiFrameworkVocab {
        /** Map: framework-package prefix → AssertionVocab */
        final Map<String, AssertionVocab> byFramework;

        MultiFrameworkVocab(Map<String, AssertionVocab> byFramework) {
            this.byFramework = Collections.unmodifiableMap(new HashMap<>(byFramework));
        }

        /** Return the vocab for an exact framework key, or empty vocab if not found. */
        AssertionVocab forFramework(String frameworkKey) {
            return byFramework.getOrDefault(frameworkKey, AssertionVocab.empty());
        }

        boolean hasFramework(String frameworkKey) {
            return byFramework.containsKey(frameworkKey);
        }

        /** True iff at least one framework has been configured. */
        boolean hasAnyVocab() {
            return !byFramework.isEmpty();
        }

        /** Return the "legacy single-vocab" — only valid if there is exactly one framework. */
        AssertionVocab singleOrEmpty() {
            if (byFramework.size() == 1) return byFramework.values().iterator().next();
            return AssertionVocab.empty();
        }
    }

    // ──────────────────────────────────────────────────────────────
    // Vocabulary loading: derive per-framework vocab from source dirs
    // ──────────────────────────────────────────────────────────────

    /**
     * Load (or derive empty) MultiFrameworkVocab for this workspace.
     * Source dirs come from .sugar/config.toml:
     *   [java-test-assertions]
     *   assertion_source_dirs = ["path/to/junit5/src", "path/to/testng/src"]
     * Each dir is parsed separately; framework is auto-detected from package names.
     */
    private static MultiFrameworkVocab loadMultiVocab(
            JavaCompiler compiler,
            Path workspaceRoot,
            List<String> diagnostics) throws IOException {

        List<Path> sourceDirs = readAssertionSourceDirs(workspaceRoot);
        Map<String, AssertionVocab> result = new HashMap<>();

        for (Path dir : sourceDirs) {
            if (!Files.isDirectory(dir)) continue;
            List<Path> frameworkSources = new ArrayList<>();
            try (Stream<Path> walk = Files.walk(dir)) {
                walk.filter(Files::isRegularFile)
                    .filter(p -> p.getFileName().toString().endsWith(".java"))
                    .sorted()
                    .forEach(frameworkSources::add);
            }
            if (frameworkSources.isEmpty()) continue;

            // Derive vocab + detect which framework it is
            VocabDeriver.DeriveResult dr = VocabDeriver.deriveFromSourcesWithFramework(
                    compiler, frameworkSources, diagnostics);
            if (dr.frameworkPackage != null) {
                // Merge into existing entry if same framework appears in multiple dirs
                AssertionVocab existing = result.get(dr.frameworkPackage);
                AssertionVocab merged = existing == null ? dr.vocab : mergeVocabs(existing, dr.vocab);
                result.put(dr.frameworkPackage, merged);
            }
        }

        // For each framework, apply the exceptions overlay
        Path excDir = workspaceRoot.resolve(".sugar").resolve("vocab-exceptions");
        for (String fw : new ArrayList<>(result.keySet())) {
            AssertionVocab overlaid = loadExceptionsOverlay(result.get(fw), excDir, diagnostics);
            result.put(fw, overlaid);
        }

        return new MultiFrameworkVocab(result);
    }

    /** Merge two AssertionVocabs (union of all sets, union of expectedArgIndex maps). */
    private static AssertionVocab mergeVocabs(AssertionVocab a, AssertionVocab b) {
        Set<String> eq = union(a.equality, b.equality);
        Set<String> ineq = union(a.inequality, b.inequality);
        Set<String> tr = union(a.truth, b.truth);
        Set<String> negTr = union(a.negatedTruth, b.negatedTruth);
        Set<String> nullS = union(a.nullSet, b.nullSet);
        Set<String> notNull = union(a.notNullSet, b.notNullSet);
        Set<String> approx = union(a.approx, b.approx);
        Set<String> unl = union(a.unlearned, b.unlearned);
        Map<String, Integer> idx = new HashMap<>(a.expectedArgIndex);
        idx.putAll(b.expectedArgIndex);
        return new AssertionVocab(
            Collections.unmodifiableSet(eq), Collections.unmodifiableSet(ineq),
            Collections.unmodifiableSet(tr), Collections.unmodifiableSet(negTr),
            Collections.unmodifiableSet(nullS), Collections.unmodifiableSet(notNull),
            Collections.unmodifiableSet(approx), Collections.unmodifiableSet(unl),
            Collections.unmodifiableMap(idx));
    }

    private static <T> Set<T> union(Set<T> a, Set<T> b) {
        Set<T> r = new HashSet<>(a); r.addAll(b); return r;
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

        // Preserve existing expectedArgIndex from base (overrides don't change order)
        return new AssertionVocab(
            Collections.unmodifiableSet(equality),
            Collections.unmodifiableSet(inequality),
            Collections.unmodifiableSet(truth),
            Collections.unmodifiableSet(negatedTruth),
            Collections.unmodifiableSet(nullSet),
            Collections.unmodifiableSet(notNullSet),
            Collections.unmodifiableSet(approx),
            Collections.unmodifiableSet(unlearned),
            base.expectedArgIndex
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
     *   equality    — assertEquals(expected, actual[, msg]); expectedArgIndex[method] says which arg is expected
     *   inequality  — assertNotEquals(unexpected, actual[, msg])
     *   truth       — assertTrue(condition[, msg])
     *   negatedTruth— assertFalse(condition[, msg])
     *   nullSet     — assertNull(actual[, msg]) => =(actual, ctor None)
     *   notNullSet  — assertNotNull(actual[, msg]) => ≠(actual, ctor None)
     *   approx      — REFUSED: carries delta/tolerance; lifting as = is a false-pass
     *   unlearned   — REFUSED: structure not understood; refuses by name
     *
     * expectedArgIndex: for each equality/inequality method, which argument index (0-based)
     *   carries the expected/unexpected (constant) value. JUnit: 0. TestNG: 1.
     *   Learned from parameter NAMES in the framework's own source.
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
        /** Maps method name → index of the expected/unexpected (constant) arg. Default: 0 (JUnit). */
        final Map<String, Integer> expectedArgIndex;

        AssertionVocab(
                Set<String> equality,
                Set<String> inequality,
                Set<String> truth,
                Set<String> negatedTruth,
                Set<String> nullSet,
                Set<String> notNullSet,
                Set<String> approx,
                Set<String> unlearned,
                Map<String, Integer> expectedArgIndex) {
            this.equality = equality; this.inequality = inequality;
            this.truth = truth; this.negatedTruth = negatedTruth;
            this.nullSet = nullSet; this.notNullSet = notNullSet;
            this.approx = approx; this.unlearned = unlearned;
            this.expectedArgIndex = expectedArgIndex;
        }

        /** Empty vocab — every assertion will be loudly refused by name. */
        static AssertionVocab empty() {
            return new AssertionVocab(
                Set.of(), Set.of(), Set.of(), Set.of(),
                Set.of(), Set.of(), Set.of(), Set.of(),
                Map.of());
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

        /**
         * Return the index (0-based) of the expected/unexpected (constant) argument for
         * this equality/inequality method. 0 = JUnit order (expected first); 1 = TestNG
         * order (actual first). Defaults to 0 if not explicitly learned.
         */
        int getExpectedArgIndex(String methodName) {
            return expectedArgIndex.getOrDefault(methodName, 0);
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
     *      → EQUALITY; expectedArgIndex learned from param[0] name:
     *        - param[0] == "expected" → JUnit order (index 0)
     *        - param[0] == "actual"   → TestNG order (index 1)
     *   3. assertNotEquals → INEQUALITY (same expected-arg-index learning applies).
     *   4. assertTrue → TRUTH.
     *   5. assertFalse → NEGATED_TRUTH.
     *   6. assertNull → NULL.
     *   7. assertNotNull → NOT_NULL.
     *   8. Everything else → UNLEARNED.
     *
     * Phase 4 addition: deriveFromSourcesWithFramework also detects which framework
     * package the source belongs to (by reading the package declaration of the parsed
     * compilation unit). This is returned alongside the vocab so the caller can key
     * the vocab by framework (e.g. "org.junit", "org.testng").
     */
    static final class VocabDeriver {

        // Parameter names that indicate approximation (must never lift as exact =).
        // This is the soundness-critical split: the signature TELLS us.
        private static final Set<String> TOLERANCE_PARAM_NAMES = Set.of(
            "delta", "tolerance", "offset", "ulps"
        );

        /** Result of per-directory vocab derivation, including detected framework package. */
        static final class DeriveResult {
            /** The derived vocab (may be empty if nothing was learned). */
            final AssertionVocab vocab;
            /**
             * The detected framework package prefix, e.g. "org.junit" or "org.testng".
             * Null if no framework package was detected.
             */
            final String frameworkPackage;

            DeriveResult(AssertionVocab vocab, String frameworkPackage) {
                this.vocab = vocab;
                this.frameworkPackage = frameworkPackage;
            }
        }

        /**
         * Derive vocabulary by parsing the given framework source files.
         * All parsing is done via JavacTask.parse() — no string scanning of source.
         * Also detects the framework package from the compilation units' package name.
         */
        static DeriveResult deriveFromSourcesWithFramework(
                JavaCompiler compiler,
                List<Path> sourceFiles,
                List<String> diagnostics) throws IOException {

            if (sourceFiles.isEmpty()) return new DeriveResult(AssertionVocab.empty(), null);

            Set<String> equality    = new HashSet<>();
            Set<String> inequality  = new HashSet<>();
            Set<String> truth       = new HashSet<>();
            Set<String> negatedTruth= new HashSet<>();
            Set<String> nullSet     = new HashSet<>();
            Set<String> notNullSet  = new HashSet<>();
            Set<String> approx      = new HashSet<>();
            Set<String> unlearned   = new HashSet<>();
            Map<String, Integer> expectedArgIndex = new HashMap<>();
            String detectedPackage = null;

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
                        // Detect framework package from the compilation unit
                        String pkg = detectPackage(unit);
                        if (pkg != null && detectedPackage == null) {
                            detectedPackage = frameworkPackageKey(pkg);
                        }
                        for (Tree decl : unit.getTypeDecls()) {
                            if (decl instanceof ClassTree ct) {
                                classifyClassMembers(ct,
                                    equality, inequality, truth, negatedTruth,
                                    nullSet, notNullSet, approx, unlearned,
                                    expectedArgIndex);
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

            AssertionVocab vocab = new AssertionVocab(
                Collections.unmodifiableSet(equality),
                Collections.unmodifiableSet(inequality),
                Collections.unmodifiableSet(truth),
                Collections.unmodifiableSet(negatedTruth),
                Collections.unmodifiableSet(nullSet),
                Collections.unmodifiableSet(notNullSet),
                Collections.unmodifiableSet(approx),
                Collections.unmodifiableSet(unlearned),
                Collections.unmodifiableMap(expectedArgIndex));
            return new DeriveResult(vocab, detectedPackage);
        }

        /**
         * Legacy entry point: derive without framework detection.
         * Kept for backward compatibility with tests that call this directly.
         */
        static AssertionVocab deriveFromSources(
                JavaCompiler compiler,
                List<Path> sourceFiles,
                List<String> diagnostics) throws IOException {
            return deriveFromSourcesWithFramework(compiler, sourceFiles, diagnostics).vocab;
        }

        /** Extract the package name string from a compilation unit tree. Returns null if none. */
        private static String detectPackage(CompilationUnitTree unit) {
            Tree pkgDecl = unit.getPackageName();
            if (pkgDecl == null) return null;
            return pkgDecl.toString();
        }

        /**
         * Map a full package name to a framework key used as the vocab map key.
         * "org.junit.*" → "org.junit"; "org.testng.*" → "org.testng".
         * Everything else → the raw package string (used as-is for unknown frameworks).
         */
        private static String frameworkPackageKey(String pkg) {
            if (pkg.startsWith("org.junit.")) return "org.junit";
            if (pkg.equals("org.junit")) return "org.junit";
            if (pkg.startsWith("org.testng.")) return "org.testng";
            if (pkg.equals("org.testng")) return "org.testng";
            return pkg;
        }

        /**
         * Walk a class's members and classify public static assert* methods.
         * This method operates purely on com.sun.source.tree.* nodes.
         */
        private static void classifyClassMembers(
                ClassTree classTree,
                Set<String> equality, Set<String> inequality,
                Set<String> truth, Set<String> negatedTruth,
                Set<String> nullSet, Set<String> notNullSet,
                Set<String> approx, Set<String> unlearned,
                Map<String, Integer> expectedArgIndex) {

            for (Tree member : classTree.getMembers()) {
                if (member instanceof MethodTree mt) {
                    classifyMethod(mt,
                        equality, inequality, truth, negatedTruth,
                        nullSet, notNullSet, approx, unlearned,
                        expectedArgIndex);
                } else if (member instanceof ClassTree nested) {
                    classifyClassMembers(nested,
                        equality, inequality, truth, negatedTruth,
                        nullSet, notNullSet, approx, unlearned,
                        expectedArgIndex);
                }
            }
        }

        /**
         * Classify one method. Structure-only: reads parameter names and types
         * from the tree, never from strings about Java semantics.
         *
         * Phase 4: also learns the expectedArgIndex by reading the NAME of the
         * first parameter:
         *   - param[0] name == "expected" → index 0 (JUnit order)
         *   - param[0] name == "actual"   → index 1 (TestNG order: actual first, expected second)
         *   - otherwise                   → index 0 (safe default)
         */
        private static void classifyMethod(
                MethodTree mt,
                Set<String> equality, Set<String> inequality,
                Set<String> truth, Set<String> negatedTruth,
                Set<String> nullSet, Set<String> notNullSet,
                Set<String> approx, Set<String> unlearned,
                Map<String, Integer> expectedArgIndex) {

            // Only classify public static assert* methods
            if (!isPublicStatic(mt)) return;
            String name = mt.getName().toString();
            if (!name.startsWith("assert")) return;

            List<? extends VariableTree> params = mt.getParameters();

            // SOUNDNESS-CRITICAL CHECK: does any parameter carry a tolerance name?
            boolean hasDeltaParam = false;
            for (VariableTree p : params) {
                String pname = p.getName().toString();
                if (TOLERANCE_PARAM_NAMES.contains(pname)) {
                    hasDeltaParam = true;
                    break;
                }
                // Also detect the 3-arg assertEquals(expected, actual, delta) shape:
                // the third parameter is a floating-point primitive (float/double).
                if (params.indexOf(p) == 2 && isFloatType(p.getType())
                        && isAssertEqualsName(name)) {
                    hasDeltaParam = true;
                    break;
                }
            }

            if (hasDeltaParam) {
                approx.add(name);
                return;
            }

            // Name-based classification
            if (isAssertEqualsName(name)) {
                equality.add(name);
                // Learn the expected-arg index from the parameter name of the FIRST param.
                // If param[0] name is "actual" → TestNG order → expected is at index 1.
                // If param[0] name is "expected" (or anything else) → JUnit order → index 0.
                // We only record if not already present (first overload wins for order learning;
                // subsequent overloads for same name should agree if they're from one framework).
                if (!expectedArgIndex.containsKey(name) && !params.isEmpty()) {
                    String firstParamName = params.get(0).getName().toString();
                    // "actual" as first param → TestNG order: const (expected) is at index 1
                    int idx = firstParamName.equals("actual") ? 1 : 0;
                    expectedArgIndex.put(name, idx);
                }
            } else if (isAssertNotEqualsName(name)) {
                inequality.add(name);
                // Same learning for assertNotEquals: param[0]=="actual" → unexpected at index 1
                if (!expectedArgIndex.containsKey(name) && !params.isEmpty()) {
                    String firstParamName = params.get(0).getName().toString();
                    int idx = firstParamName.equals("actual") ? 1 : 0;
                    expectedArgIndex.put(name, idx);
                }
            } else if (isAssertTrueName(name)) {
                truth.add(name);
            } else if (isAssertFalseName(name)) {
                negatedTruth.add(name);
            } else if (isAssertNullName(name)) {
                nullSet.add(name);
            } else if (isAssertNotNullName(name)) {
                notNullSet.add(name);
            } else {
                unlearned.add(name);
            }
        }

        // ── classification pattern predicates ──────────────────────────────────

        private static boolean isAssertEqualsName(String name) {
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

        private static boolean isFloatType(Tree typeTree) {
            if (typeTree instanceof PrimitiveTypeTree ptt) {
                TypeKind kind = ptt.getPrimitiveTypeKind();
                return kind == TypeKind.FLOAT || kind == TypeKind.DOUBLE;
            }
            return false;
        }
    }

    // ──────────────────────────────────────────────────────────────
    // Framework detection for a source file
    // ──────────────────────────────────────────────────────────────

    /**
     * Result of framework detection for a single source file.
     * Carries the resolved vocab to use (or null for ambiguity/no-vocab cases).
     */
    private enum FrameworkKind {
        JUNIT,    // org.junit.* imports only
        TESTNG,   // org.testng.* imports only
        BOTH,     // both org.junit.Assert AND org.testng.Assert imported → ambiguous
        NEITHER   // no assertion framework imports detected
    }

    /**
     * Detect which assertion framework(s) a compilation unit imports.
     * Rules (Phase 4):
     *   - Import of org.junit.Assert or org.junit.jupiter.api.Assertions (direct or static)
     *     → JUnit assertion class present
     *   - Import of org.testng.Assert (direct or static)
     *     → TestNG assertion class present
     *   - BOTH Assert classes imported → BOTH (ambiguous)
     *   - Only one → that framework
     *   - @Test annotation from org.testng.annotations is a marker for TestNG @Test,
     *     but does NOT by itself count as an assertion-vocab conflict (TestNG tests can
     *     call JUnit assertions). The assertion class import is the discriminator.
     */
    private static FrameworkKind detectFrameworkKind(CompilationUnitTree unit) {
        boolean hasJUnitAssert = false;
        boolean hasTestNGAssert = false;
        for (ImportTree imp : unit.getImports()) {
            String name = imp.getQualifiedIdentifier().toString();
            // JUnit assertion imports (direct or static)
            if (name.equals("org.junit.Assert")
                    || name.startsWith("org.junit.Assert.")
                    || name.equals("org.junit.jupiter.api.Assertions")
                    || name.startsWith("org.junit.jupiter.api.Assertions.")
                    || name.startsWith("org.junit.Assert.*")) {
                hasJUnitAssert = true;
            }
            // TestNG assertion imports (direct or static)
            if (name.equals("org.testng.Assert")
                    || name.startsWith("org.testng.Assert.")
                    || name.startsWith("org.testng.Assert.*")) {
                hasTestNGAssert = true;
            }
        }
        if (hasJUnitAssert && hasTestNGAssert) return FrameworkKind.BOTH;
        if (hasJUnitAssert) return FrameworkKind.JUNIT;
        if (hasTestNGAssert) return FrameworkKind.TESTNG;
        // Fallback: check for bare org.junit.* imports (covers JUnit 4 @Test + Assert usages)
        for (ImportTree imp : unit.getImports()) {
            String name = imp.getQualifiedIdentifier().toString();
            if (name.startsWith("org.junit.")) return FrameworkKind.JUNIT;
        }
        return FrameworkKind.NEITHER;
    }

    /**
     * Select the AssertionVocab to use for a file, given the detected framework.
     * Returns null when the framework is BOTH (ambiguity → refuse all assertions by name).
     * Returns empty vocab when framework is NEITHER or vocab is not configured.
     */
    private static AssertionVocab selectVocabForFramework(
            FrameworkKind kind,
            MultiFrameworkVocab multiVocab) {
        return switch (kind) {
            case JUNIT  -> multiVocab.forFramework("org.junit");
            case TESTNG -> multiVocab.forFramework("org.testng");
            case BOTH   -> null;  // ambiguous: refuse all assertions
            case NEITHER -> AssertionVocab.empty(); // no vocab context
        };
    }

    // ──────────────────────────────────────────────────────────────
    // Per-file lift using javac parse-only tree walk
    // ──────────────────────────────────────────────────────────────

    private static void liftFile(
            JavaCompiler compiler,
            Path abs,
            String rel,
            MultiFrameworkVocab multiVocab,
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
            FrameworkKind frameworkKind = detectFrameworkKind(unit);
            AssertionVocab vocab = selectVocabForFramework(frameworkKind, multiVocab);

            // vocab == null means BOTH frameworks imported → ambiguous
            boolean ambiguousFramework = (vocab == null);
            if (ambiguousFramework) {
                // We still need A vocab to walk the tree (so we can emit refusals per-method).
                // Use empty vocab — the ambiguity flag will override all assertions.
                vocab = AssertionVocab.empty();
            }

            for (Tree decl : unit.getTypeDecls()) {
                if (decl instanceof ClassTree ct) {
                    walkClassMembers(ct, unit, rel, importedNames, vocab,
                            frameworkKind, ambiguousFramework, ir, diagnostics, null);
                }
            }
        }
        fm.close();
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
            FrameworkKind frameworkKind,
            boolean ambiguousFramework,
            List<String> ir,
            List<String> diagnostics,
            String outerClassName) {

        String className = classTree.getSimpleName().toString();
        if (outerClassName != null) className = outerClassName + "." + className;

        for (Tree member : classTree.getMembers()) {
            if (member instanceof MethodTree mt) {
                liftMethod(mt, unit, rel, className, importedNames, vocab,
                        frameworkKind, ambiguousFramework, ir, diagnostics);
            } else if (member instanceof ClassTree nested) {
                walkClassMembers(nested, unit, rel, importedNames, vocab,
                        frameworkKind, ambiguousFramework, ir, diagnostics, className);
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
            FrameworkKind frameworkKind,
            boolean ambiguousFramework,
            List<String> ir,
            List<String> diagnostics) {

        if (!hasTestAnnotation(method, importedNames)) return;

        String methodName = method.getName().toString();
        String scope = rel + "::" + className + "::" + methodName;

        BlockTree body = method.getBody();
        if (body == null) return;

        Set<String> mutatedLocals = computeMutatedLocals(body);

        for (StatementTree stmt : body.getStatements()) {
            if (stmt instanceof ExpressionStatementTree est) {
                liftStatement(est.getExpression(), scope, vocab, frameworkKind,
                        ambiguousFramework, ir, diagnostics);
            } else if (stmt instanceof ForLoopTree flt) {
                liftForLoop(flt, scope, vocab, ambiguousFramework, mutatedLocals, ir, diagnostics);
            }
        }
    }

    // ──────────────────────────────────────────────────────────────
    // Final-oracle: compute the set of locally-mutated variable names
    // ──────────────────────────────────────────────────────────────

    private static Set<String> computeMutatedLocals(BlockTree body) {
        Set<String> mutated = new HashSet<>();
        scanForMutations(body.getStatements(), mutated);
        return Collections.unmodifiableSet(mutated);
    }

    private static void scanForMutations(Iterable<? extends StatementTree> stmts, Set<String> out) {
        for (StatementTree stmt : stmts) {
            scanStmtForMutations(stmt, out);
        }
    }

    private static void scanStmtForMutations(StatementTree stmt, Set<String> out) {
        if (stmt == null) return;
        if (stmt instanceof ExpressionStatementTree est) {
            scanExprForMutations(est.getExpression(), out);
        } else if (stmt instanceof ForLoopTree flt) {
            for (StatementTree init : flt.getInitializer()) {
                scanStmtForMutations(init, out);
            }
            scanExprForMutations(flt.getCondition(), out);
            for (ExpressionStatementTree upd : flt.getUpdate()) {
                scanExprForMutations(upd.getExpression(), out);
            }
            scanStmtForMutations(flt.getStatement(), out);
        } else if (stmt instanceof BlockTree bt) {
            scanForMutations(bt.getStatements(), out);
        } else if (stmt instanceof VariableTree vt) {
            scanExprForMutations(vt.getInitializer(), out);
        } else if (stmt instanceof IfTree it) {
            scanExprForMutations(it.getCondition(), out);
            scanStmtForMutations(it.getThenStatement(), out);
            scanStmtForMutations(it.getElseStatement(), out);
        } else if (stmt instanceof WhileLoopTree wlt) {
            scanExprForMutations(wlt.getCondition(), out);
            scanStmtForMutations(wlt.getStatement(), out);
        } else if (stmt instanceof ReturnTree rt) {
            scanExprForMutations(rt.getExpression(), out);
        }
    }

    private static void scanExprForMutations(ExpressionTree expr, Set<String> out) {
        if (expr == null) return;
        if (expr instanceof AssignmentTree at) {
            ExpressionTree var = at.getVariable();
            if (var instanceof IdentifierTree id) {
                out.add(id.getName().toString());
            }
            scanExprForMutations(at.getExpression(), out);
        } else if (expr instanceof CompoundAssignmentTree cat) {
            ExpressionTree var = cat.getVariable();
            if (var instanceof IdentifierTree id) {
                out.add(id.getName().toString());
            }
            scanExprForMutations(cat.getExpression(), out);
        } else if (expr instanceof UnaryTree ut) {
            Tree.Kind kind = ut.getKind();
            if (kind == Tree.Kind.PREFIX_INCREMENT || kind == Tree.Kind.PREFIX_DECREMENT
                    || kind == Tree.Kind.POSTFIX_INCREMENT || kind == Tree.Kind.POSTFIX_DECREMENT) {
                ExpressionTree operand = ut.getExpression();
                if (operand instanceof IdentifierTree id) {
                    out.add(id.getName().toString());
                }
            }
            scanExprForMutations(ut.getExpression(), out);
        } else if (expr instanceof MethodInvocationTree mit) {
            for (ExpressionTree arg : mit.getArguments()) {
                scanExprForMutations(arg, out);
            }
        } else if (expr instanceof BinaryTree bt2) {
            scanExprForMutations(bt2.getLeftOperand(), out);
            scanExprForMutations(bt2.getRightOperand(), out);
        } else if (expr instanceof ParenthesizedTree pt) {
            scanExprForMutations(pt.getExpression(), out);
        }
    }

    // ──────────────────────────────────────────────────────────────
    // Loop→∀ lifter
    // ──────────────────────────────────────────────────────────────

    private static void liftForLoop(
            ForLoopTree flt,
            String scope,
            AssertionVocab vocab,
            boolean ambiguousFramework,
            Set<String> methodMutatedLocals,
            List<String> ir,
            List<String> diagnostics) {

        List<? extends StatementTree> inits = flt.getInitializer();
        if (inits.size() != 1 || !(inits.get(0) instanceof VariableTree vt)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>", "loop→∀ refused: init is not a single variable declaration"));
            return;
        }
        String loopVar = vt.getName().toString();
        ExpressionTree initExpr = vt.getInitializer();
        OptionalLong loStart = asIntLiteral(initExpr);
        if (loStart.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>", "loop→∀ refused: loop init is not an int literal (open lower bound)"));
            return;
        }
        long startVal = loStart.getAsLong();

        ExpressionTree cond = flt.getCondition();
        if (!(cond instanceof BinaryTree bt)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>", "loop→∀ refused: condition is not a binary comparison"));
            return;
        }
        Tree.Kind condKind = bt.getKind();
        if (condKind != Tree.Kind.LESS_THAN && condKind != Tree.Kind.LESS_THAN_EQUAL) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>", "loop→∀ refused: condition operator is not < or <="));
            return;
        }
        if (!(bt.getLeftOperand() instanceof IdentifierTree condId)
                || !condId.getName().toString().equals(loopVar)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>", "loop→∀ refused: condition left side is not the loop variable"));
            return;
        }
        OptionalLong hiOpt = asIntLiteral(bt.getRightOperand());
        if (hiOpt.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>", "loop→∀ refused: loop bound is not an int literal (open upper bound — would produce open forall)"));
            return;
        }
        long endVal = hiOpt.getAsLong();
        boolean inclusive = (condKind == Tree.Kind.LESS_THAN_EQUAL);

        List<? extends ExpressionStatementTree> updates = flt.getUpdate();
        if (updates.size() != 1) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>", "loop→∀ refused: update clause must have exactly one expression"));
            return;
        }
        ExpressionTree updateExpr = updates.get(0).getExpression();
        if (!isSimpleIncrement(updateExpr, loopVar)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>", "loop→∀ refused: update is not <var>++ / ++<var> / <var>+=1"));
            return;
        }

        StatementTree bodyStmt = flt.getStatement();
        List<MethodInvocationTree> bodyAsserts = new ArrayList<>();
        if (!collectBodyAsserts(bodyStmt, loopVar, bodyAsserts)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>", "loop→∀ refused: loop body contains non-assertion statements"));
            return;
        }
        if (bodyAsserts.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>", "loop→∀ refused: loop body has no assertions"));
            return;
        }

        Set<String> bodyMutated = new HashSet<>();
        scanStmtForMutations(bodyStmt, bodyMutated);
        if (bodyMutated.contains(loopVar)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>",
                "loop→∀ refused: body mutates the loop variable " + loopVar
                    + " (iteration space not the stated range — universal would be false)"));
            return;
        }
        if (!bodyMutated.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                "<loop>",
                "loop→∀ refused: body mutates " + bodyMutated + " (accumulator pattern — universal would be false)"));
            return;
        }

        List<String> bodyFormulas = new ArrayList<>();
        for (MethodInvocationTree mit : bodyAsserts) {
            String assertName = methodInvocationName(mit);
            String category = vocab.classify(assertName);
            String formula = tryLiftBodyAssertion(mit, assertName, category, loopVar, vocab, scope, diagnostics);
            if (formula == null) {
                return;
            }
            bodyFormulas.add(formula);
        }

        String contractName = scope + "::loop::" + loopVar;
        ir.add(buildForallContract(contractName, loopVar, startVal, endVal, inclusive, bodyFormulas));
    }

    private static boolean isSimpleIncrement(ExpressionTree expr, String varName) {
        if (expr instanceof UnaryTree ut) {
            Tree.Kind k = ut.getKind();
            if (k == Tree.Kind.POSTFIX_INCREMENT || k == Tree.Kind.PREFIX_INCREMENT) {
                return (ut.getExpression() instanceof IdentifierTree id)
                        && id.getName().toString().equals(varName);
            }
        }
        if (expr instanceof CompoundAssignmentTree cat) {
            if (cat.getKind() == Tree.Kind.PLUS_ASSIGNMENT) {
                if (!(cat.getVariable() instanceof IdentifierTree id)
                        || !id.getName().toString().equals(varName)) return false;
                OptionalLong step = asIntLiteral(cat.getExpression());
                return step.isPresent() && step.getAsLong() == 1L;
            }
        }
        return false;
    }

    private static boolean collectBodyAsserts(StatementTree stmt, String loopVar,
                                              List<MethodInvocationTree> out) {
        if (stmt instanceof BlockTree bt) {
            for (StatementTree s : bt.getStatements()) {
                if (!collectBodyAsserts(s, loopVar, out)) return false;
            }
            return true;
        }
        if (stmt instanceof ExpressionStatementTree est) {
            ExpressionTree expr = est.getExpression();
            if (expr instanceof MethodInvocationTree mit) {
                out.add(mit);
                return true;
            }
            return false;
        }
        return false;
    }

    private static String tryLiftBodyAssertion(
            MethodInvocationTree mit,
            String assertName,
            String category,
            String loopVar,
            AssertionVocab vocab,
            String scope,
            List<String> diagnostics) {

        if (!category.equals("equality")) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                assertName, "loop→∀ body assertion not liftable (only equality assertions supported in loop body): " + assertName));
            return null;
        }

        List<? extends ExpressionTree> args = mit.getArguments();
        if (args.size() < 2) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                assertName, "loop→∀ body: " + assertName + " arity " + args.size() + " < 2"));
            return null;
        }

        // Use the learned expectedArgIndex to know which arg is the constant.
        int constIdx = vocab.getExpectedArgIndex(assertName);
        int callIdx = 1 - constIdx; // the other arg must be the call expression

        ExpressionTree constExpr = args.get(constIdx);
        ExpressionTree callExpr  = args.get(callIdx);

        OptionalLong constVal = asIntLiteral(constExpr);
        if (constVal.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                assertName, "loop→∀ body: expected (constant) arg[" + constIdx + "] is not an int literal: " + constExpr));
            return null;
        }

        if (!(callExpr instanceof MethodInvocationTree callMit)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                assertName, "loop→∀ body: actual (call) arg[" + callIdx + "] is not a method call: " + callExpr));
            return null;
        }

        String callee = methodInvocationName(callMit);
        if (callee.contains(".")) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                assertName, "loop→∀ body: callee is qualified (" + callee + "); only bare function names lifted"));
            return null;
        }

        List<? extends ExpressionTree> callArgs = callMit.getArguments();
        List<String> argJsons = new ArrayList<>();
        for (ExpressionTree a : callArgs) {
            if (a instanceof IdentifierTree id && id.getName().toString().equals(loopVar)) {
                argJsons.add("{\"kind\":\"var\",\"name\":\"" + esc(loopVar) + "\"}");
            } else {
                OptionalLong val = asIntLiteral(a);
                if (val.isEmpty()) {
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        assertName, "loop→∀ body: call arg is not the loop variable or an int literal: " + a));
                    return null;
                }
                argJsons.add("{\"kind\":\"const\",\"value\":" + val.getAsLong()
                        + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}");
            }
        }

        String ctorArgs = String.join(",", argJsons);
        String ctorJson = "{\"kind\":\"ctor\",\"name\":\"call:" + esc(callee) + "\",\"args\":["
                + ctorArgs + "]}";
        String constJson = "{\"kind\":\"const\",\"value\":" + constVal.getAsLong()
                + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";
        return "{\"kind\":\"atomic\",\"name\":\"=\",\"args\":[" + ctorJson + "," + constJson + "]}";
    }

    private static String buildForallContract(
            String contractName,
            String var,
            long startVal,
            long endVal,
            boolean inclusive,
            List<String> bodyFormulas) {

        String varRef = "{\"kind\":\"var\",\"name\":\"" + esc(var) + "\"}";
        String startConst = "{\"kind\":\"const\",\"value\":" + startVal
                + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";
        String endConst = "{\"kind\":\"const\",\"value\":" + endVal
                + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";

        String lowerAtom = "{\"kind\":\"atomic\",\"name\":\"≤\",\"args\":["
                + startConst + "," + varRef + "]}";
        String upperOp = inclusive ? "≤" : "<";
        String upperAtom = "{\"kind\":\"atomic\",\"name\":\"" + upperOp + "\",\"args\":["
                + varRef + "," + endConst + "]}";

        String guard = "{\"kind\":\"and\",\"operands\":[" + lowerAtom + "," + upperAtom + "]}";
        String bodyConj = "{\"kind\":\"and\",\"operands\":[" + String.join(",", bodyFormulas) + "]}";
        String implies = "{\"kind\":\"implies\",\"operands\":[" + guard + "," + bodyConj + "]}";
        String forall = "{\"kind\":\"forall\",\"name\":\"" + esc(var)
                + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"body\":" + implies + "}";

        return "{\"kind\":\"contract\""
             + ",\"name\":\"" + esc(contractName) + "\""
             + ",\"outBinding\":\"out\""
             + ",\"inv\":{\"kind\":\"and\",\"operands\":[" + forall + "]}}";
    }

    // ──────────────────────────────────────────────────────────────
    // Determine if a method has @Test (JUnit 4, JUnit 5, or TestNG)
    // ──────────────────────────────────────────────────────────────

    private static boolean hasTestAnnotation(MethodTree method, Set<String> importedNames) {
        for (AnnotationTree ann : method.getModifiers().getAnnotations()) {
            String typeName = ann.getAnnotationType().toString();
            if (typeName.equals("Test")
                    || typeName.equals("org.junit.Test")
                    || typeName.equals("org.junit.jupiter.api.Test")
                    || typeName.equals("org.testng.annotations.Test")) {
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
            AssertionVocab vocab,
            FrameworkKind frameworkKind,
            boolean ambiguousFramework,
            List<String> ir,
            List<String> diagnostics) {

        if (!(expr instanceof MethodInvocationTree mit)) return;

        String methodName = methodInvocationName(mit);
        if (!methodName.startsWith("assert")) return;

        // AMBIGUITY REFUSAL: both JUnit and TestNG Assert imported.
        // The vocabulary order is undefined → refuse all assertions by name.
        if (ambiguousFramework) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName,
                "ambiguous assertion vocabulary: file imports both org.junit.Assert and org.testng.Assert; "
                + "argument order is undefined — refused to avoid mis-lift: " + methodName));
            return;
        }

        String category = vocab.classify(methodName);

        switch (category) {
            case "approx" -> {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName,
                    "approximate assertion (delta) is not exact equality; refused to avoid false-pass"));
            }
            case "unlearned" -> {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName,
                    "assertion not in learned vocabulary; refused by name: " + methodName));
            }
            case "unknown" -> {
                if (vocab.equality.isEmpty() && vocab.inequality.isEmpty()
                        && vocab.truth.isEmpty() && vocab.nullSet.isEmpty()) {
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
            case "inequality" -> liftInequality(mit, methodName, scope, vocab, ir, diagnostics);
            case "truth" -> liftTruth(mit, methodName, scope, ir, diagnostics);
            case "negated_truth" -> liftNegatedTruth(mit, methodName, scope, ir, diagnostics);
            case "null" -> liftNull(mit, methodName, scope, ir, diagnostics);
            case "not_null" -> liftNotNull(mit, methodName, scope, ir, diagnostics);
        }
    }

    // ──────────────────────────────────────────────────────────────
    // Category-specific lift methods
    // ──────────────────────────────────────────────────────────────

    /**
     * Lift assertEquals.
     * Phase 4: uses vocab.getExpectedArgIndex(methodName) to determine which
     * argument is the expected (constant) value. JUnit: index 0; TestNG: index 1.
     * This is the ONLY place the argument order matters — learned from source,
     * never hardcoded per-framework here.
     */
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

        // Learned: which argument index holds the expected/constant value
        int constIdx = vocab.getExpectedArgIndex(methodName);
        int callIdx  = 1 - constIdx;

        ExpressionTree expectedExpr = args.get(constIdx);
        ExpressionTree actualExpr   = args.get(callIdx);

        if (args.size() == 3) {
            // 3-arg form. Possible shapes vary by framework:
            // JUnit: (expected, actual, message[String]) or (expected, actual, delta[float])
            // TestNG: (actual, expected, message[String]) or (actual, expected, delta[float])
            // We handle message-first as a special case for JUnit (message is arg[0] when constIdx==0).
            // For TestNG (constIdx==1), the message is arg[2].
            if (constIdx == 0) {
                // JUnit layout: args[0]=expected, args[1]=actual, args[2]=msg|delta
                ExpressionTree arg0 = args.get(0);
                ExpressionTree arg2 = args.get(2);
                if (arg0 instanceof LiteralTree lt0 && lt0.getValue() instanceof String) {
                    // (String msg, expected, actual)
                    expectedExpr = args.get(1);
                    actualExpr   = args.get(2);
                } else if (vocab.hasApproxOverload(methodName) && isNumericLiteral(arg2)) {
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName,
                        "approximate assertion (delta) is not exact equality; refused to avoid false-pass"));
                    return;
                } else {
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName, "3-arg " + methodName + " with non-string first arg not lifted"));
                    return;
                }
            } else {
                // TestNG layout: args[0]=actual, args[1]=expected, args[2]=msg|delta
                ExpressionTree arg2 = args.get(2);
                if (vocab.hasApproxOverload(methodName) && isNumericLiteral(arg2)) {
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName,
                        "approximate assertion (delta) is not exact equality; refused to avoid false-pass"));
                    return;
                } else if (arg2 instanceof LiteralTree lt2 && lt2.getValue() instanceof String) {
                    // (actual, expected, String msg) — message last, keep current ordering
                    // expectedExpr and actualExpr already set correctly above
                } else {
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName, "3-arg " + methodName + " not lifted (unknown 3-arg shape)"));
                    return;
                }
            }
        } else if (args.size() > 3) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + " arity " + args.size() + " not lifted"));
            return;
        }

        liftBinaryIntContract(expectedExpr, actualExpr, "=", methodName, scope, ir, diagnostics);
    }

    private static boolean isNumericLiteral(ExpressionTree expr) {
        if (expr instanceof LiteralTree lt) {
            return lt.getValue() instanceof Number;
        }
        return false;
    }

    /**
     * Lift assertNotEquals.
     * Phase 4: uses vocab.getExpectedArgIndex to determine which arg is the unexpected constant.
     */
    private static void liftInequality(
            MethodInvocationTree mit, String methodName, String scope,
            AssertionVocab vocab,
            List<String> ir, List<String> diagnostics) {

        List<? extends ExpressionTree> args = mit.getArguments();
        if (args.size() < 2) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + " arity " + args.size() + " < 2"));
            return;
        }

        int constIdx = vocab.getExpectedArgIndex(methodName);
        int callIdx  = 1 - constIdx;

        ExpressionTree unexpectedExpr = args.get(constIdx);
        ExpressionTree actualExpr     = args.get(callIdx);

        if (args.size() == 3) {
            if (constIdx == 0) {
                if (args.get(0) instanceof LiteralTree lt0 && lt0.getValue() instanceof String) {
                    unexpectedExpr = args.get(1);
                    actualExpr     = args.get(2);
                } else {
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName, "3-arg " + methodName + " with non-string first arg not lifted"));
                    return;
                }
            } else {
                // TestNG: (actual, unexpected, msg)
                ExpressionTree arg2 = args.get(2);
                if (!(arg2 instanceof LiteralTree lt2 && lt2.getValue() instanceof String)) {
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName, "3-arg " + methodName + " not lifted (unknown 3-arg shape)"));
                    return;
                }
            }
        } else if (args.size() > 3) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + " arity " + args.size() + " not lifted"));
            return;
        }

        liftBinaryIntContract(unexpectedExpr, actualExpr, "≠", methodName, scope, ir, diagnostics);
    }

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

        ir.add(buildTruthContract(callee, argValues, true));
    }

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

    /** Kept for backward compatibility */
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
    // File enumeration
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
    // Minimal JSON-RPC wire codec (operates on JSON wire bytes only)
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
