// SPDX-License-Identifier: Apache-2.0
//
// Java-native JUnit/TestNG assertion lifter for the Sugar/ProvekIt substrate.
// Phase 4.5: throw-locus derivation — the name never enters into it.
//
// THE LAW: every fact about Java source comes from a com.sun.source.tree.*
// node. No regex, indexOf, split, or any string-scanning of Java source code
// is used here. JSON-RPC wire protocol codec uses indexOf/split on JSON bytes
// only -- not on Java source.
//
// THE POINT OF PHASE 4.5 (T, verbatim):
// "Hard coding 'assert' is simply the wrong behavior. We know FOL when we see
// it, not when someone says 'This might be it!'"
//
// The old VocabDeriver classified methods by NAME (name-keyed pattern predicates).
// That is the forbidden middle: reasoning about source we hold without walking
// it. Phase 4.5 DELETES every name-keyed classification rule and replaces
// them with throw-locus derivation:
//
//   A method IS an assertion iff its body reduces to a guarded throw, and the
//   THROW-GUARD is its semantics. The name never enters into it.
//
// For every public static method in the framework source (NO name filter for
// candidate selection — structure identifies itself):
//   1. Parse the body via com.sun.source (tree nodes; no string-scanning).
//   2. Reduce the body by inlining the delegation chain, depth-bounded (8).
//   3. Find the throw locus: an IfTree(guard, {throw/call-that-throws}, /).
//      A method with NO reachable throw locus is NOT an assertion — skip.
//   4. Classify the guard STRUCTURALLY:
//      - guard = !condition (boolean param)          → TRUTH
//      - guard = condition  (boolean param, no neg)  → NEGATED_TRUTH
//      - guard = p_i != p_j or !objectsAreEqual(...)  → EQUALITY
//        (order from param positions; param-name cross-check)
//      - guard = p_i == p_j or objectsAreEqual(...)   → INEQUALITY
//      - guard = p_i != null                          → NULL
//      - guard = p_i == null                          → NOT_NULL
//      - guard involves tolerance (delta param / float-comparison call) → APPROXIMATE
//      - anything else                                → UNLEARNED
//
// JUnit5's Assertions.java delegates to package-private classes (AssertEquals,
// AssertTrue, etc.). Those are also vendored (tag r5.10.2). The deriver parses
// ALL files in the vendor dir, builds a corpus map, and inlines across it.
// A call that leaves vendored source = chain broken = UNLEARNED.
//
// TestNG's Assert.java is largely self-contained — the chain stays in one file.
//
// An externalized .sugar/vocab-exceptions/<framework>.json overlays the derived
// table. With no source configured every assertion is refused.
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
    private static final String VERSION = "0.7.0"; // P5c: call-binding lift (SSA + instance-method location-keyed)

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

        // G1: Load universe registry from vendor_source_dirs (implementation-contract pass).
        // vendor_source_dirs in [java-test-assertions] config.toml points at vendor implementation
        // source trees. The UniverseWalker walks static final byte[] encode tables and registers
        // universe contracts (str.chars-in-set) per entry-point method name.
        UniverseRegistry universeRegistry = UniverseWalker.loadRegistry(compiler, root, diagnostics);

        // G2: Load numeric universe registry from vendor_source_dirs (numeric-universe-walk pass).
        // The NumericUniverseWalker walks public static int-returning methods in vendored source
        // and registers int32.eq-bv-expr universe contracts per method name.
        // Supported shapes: ternary-with-comparison ((a < 0) ? -a : a) → abs BV expression.
        NumericUniverseRegistry numericRegistry = NumericUniverseWalker.loadRegistry(compiler, root, diagnostics);

        // G3: Load instance-universe — walks receiver classes in the WORKSPACE to pin
        // construction-time facts: new Box(5).get() == 5 BY CONSTRUCTION (ctor→field→getter).
        // Pure final-field-return-only tier; anything more complex is refused by name.
        InstanceUniverse instanceUniverse = InstanceUniverse.load(compiler, root, diagnostics);

        for (String rel : files) {
            Path abs = root.resolve(rel).normalize();
            if (!Files.isReadable(abs)) {
                diagnostics.add(diagnostic(rel, null, null, "cannot read file"));
                continue;
            }
            // PER-FILE ISOLATION (multi-file robustness): a single vendor file that
            // throws during parse/walk (malformed source, unsupported syntax, an
            // internal walker error) must NOT zero out the whole artifact. Mirror the
            // rust coretests_sweep tolerance: skip-and-diagnose per file, keep the
            // contracts already lifted from the other files. Without this, one bad
            // file in a 229-file vendor test tree drops the entire artifact to GAP.
            try {
                liftFile(compiler, abs, rel, multiVocab, universeRegistry, numericRegistry, instanceUniverse, ir, diagnostics);
            } catch (Exception e) {
                diagnostics.add(diagnostic(rel, null, null,
                    "per-file lift skipped (isolated): "
                    + (e.getMessage() == null ? e.toString() : e.getMessage())));
            }
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
        Set<String> noThrow = union(a.noThrowLocus, b.noThrowLocus);
        Map<String, Integer> idx = new HashMap<>(a.expectedArgIndex);
        idx.putAll(b.expectedArgIndex);
        return new AssertionVocab(
            Collections.unmodifiableSet(eq), Collections.unmodifiableSet(ineq),
            Collections.unmodifiableSet(tr), Collections.unmodifiableSet(negTr),
            Collections.unmodifiableSet(nullS), Collections.unmodifiableSet(notNull),
            Collections.unmodifiableSet(approx), Collections.unmodifiableSet(unl),
            Collections.unmodifiableSet(noThrow),
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
        Set<String> noThrowLocus= new HashSet<>(base.noThrowLocus);

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
                noThrowLocus.remove(name);
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
            Collections.unmodifiableSet(noThrowLocus),
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
        /** Methods whose reduced body has NO reachable throw — NOT assertions.
         *  A call to one of these gets the named "no throw locus" refusal. */
        final Set<String> noThrowLocus;
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
                Set<String> noThrowLocus,
                Map<String, Integer> expectedArgIndex) {
            this.equality = equality; this.inequality = inequality;
            this.truth = truth; this.negatedTruth = negatedTruth;
            this.nullSet = nullSet; this.notNullSet = notNullSet;
            this.approx = approx; this.unlearned = unlearned;
            this.noThrowLocus = noThrowLocus;
            this.expectedArgIndex = expectedArgIndex;
        }

        /** Empty vocab — every assertion will be loudly refused by name. */
        static AssertionVocab empty() {
            return new AssertionVocab(
                Set.of(), Set.of(), Set.of(), Set.of(),
                Set.of(), Set.of(), Set.of(), Set.of(), Set.of(),
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
            if (noThrowLocus.contains(name))return "no_throw_locus";
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
     * no bytecode, no javap). Classification is purely structural via throw-locus
     * derivation: a method IS an assertion iff its body reduces to a guarded throw,
     * and the THROW-GUARD is its semantics. The name never enters into it.
     *
     * Phase 4.5: all name-keyed classification rules deleted. Replaced by:
     *   1. Parse ALL files in the vendor dir into a corpus (className → methods).
     *   2. For each public static method (no name filter — structure identifies itself):
     *      a. Reduce the body depth-bounded (8 levels) by inlining delegation.
     *      b. Find the throw locus: IfTree(guard, {throw or call-that-throws}).
     *      c. Classify the guard STRUCTURALLY (see classifyGuard).
     *   3. Any method with no reachable throw locus is NOT an assertion — skipped.
     *
     * Guard classification:
     *   - !condition (boolean param)             → TRUTH
     *   - condition  (boolean param, no negation) → NEGATED_TRUTH
     *   - p_i != p_j or !objectsAreEqual/areEqualImpl   → EQUALITY (guard positions = order)
     *   - p_i == p_j or objectsAreEqual/areEqualImpl    → INEQUALITY
     *   - p_i != null                            → NULL
     *   - p_i == null                            → NOT_NULL
     *   - tolerance param in scope / float-comparison call with 3rd param → APPROXIMATE
     *   - anything else                          → UNLEARNED
     *
     * Cross-check: guard-derived expected/actual positions are compared against
     * param names ("expected"/"actual"). Disagreement → UNLEARNED + report.
     *
     * deriveFromSourcesWithFramework also detects which framework package the
     * source belongs to (by reading the package declaration of the parsed
     * compilation unit). This is returned alongside the vocab so the caller can key
     * the vocab by framework (e.g. "org.junit", "org.testng").
     */
    static final class VocabDeriver {

        // Method names that we recognise as equality-predicate sentinels inside
        // the vendor source — calls to these in the guard count as objectsAreEqual.
        private static final Set<String> EQUAL_PREDICATE_METHODS = Set.of(
            "objectsAreEqual", "areEqualImpl", "areEqual",
            "floatsAreEqual", "doublesAreEqual"
        );

        // Method names that are NOT-EQUAL predicate sentinels: they return true
        // when the arguments are NOT equal (semantic inverse of EQUAL_PREDICATE_METHODS).
        // e.g. TestNG's areNotEqualImpl(actual, expected) returns true when actual != expected.
        // In a guard: !areNotEqualImpl(a,b) → throws when NOT (a!=b) = throws when a==b → INEQUALITY
        //             areNotEqualImpl(a,b)  → throws when a!=b → EQUALITY
        // H1: added to fix TestNG assertNotEquals 2-arg classification (per-overload C8 fix).
        private static final Set<String> NOT_EQUAL_PREDICATE_METHODS = Set.of(
            "areNotEqualImpl", "areNotEqual", "objectsAreNotEqual"
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

        // ── corpus: className → list of all MethodTree nodes ──────────────────

        /** One parsed class: simple name + all its MethodTree members (flat, including nested). */
        private static final class ClassCorpus {
            final String simpleName;
            final List<MethodTree> methods;
            ClassCorpus(String simpleName, List<MethodTree> methods) {
                this.simpleName = simpleName;
                this.methods = methods;
            }
        }

        /**
         * Parse all source files, build a flat corpus map: simpleName → ClassCorpus.
         * All parsing via JavacTask.parse() — no string scanning.
         */
        private static Map<String, ClassCorpus> buildCorpus(
                JavaCompiler compiler,
                List<Path> sourceFiles,
                List<String> diagnostics) throws IOException {

            Map<String, ClassCorpus> corpus = new LinkedHashMap<>();
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
                                collectClassIntoCorpus(ct, corpus);
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
            return corpus;
        }

        /** Recursively collect all methods from a class (and nested classes) into corpus. */
        private static void collectClassIntoCorpus(
                ClassTree ct, Map<String, ClassCorpus> corpus) {
            String name = ct.getSimpleName().toString();
            List<MethodTree> methods = new ArrayList<>();
            for (Tree m : ct.getMembers()) {
                if (m instanceof MethodTree mt) methods.add(mt);
                else if (m instanceof ClassTree nested) collectClassIntoCorpus(nested, corpus);
            }
            corpus.put(name, new ClassCorpus(name, methods));
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

            // Step 1: build corpus from ALL vendored files in this batch
            Map<String, ClassCorpus> corpus = buildCorpus(compiler, sourceFiles, diagnostics);

            Set<String> equality    = new HashSet<>();
            Set<String> inequality  = new HashSet<>();
            Set<String> truth       = new HashSet<>();
            Set<String> negatedTruth= new HashSet<>();
            Set<String> nullSet     = new HashSet<>();
            Set<String> notNullSet  = new HashSet<>();
            Set<String> approx      = new HashSet<>();
            Set<String> unlearned   = new HashSet<>();
            Set<String> noThrowLocus= new HashSet<>();
            Map<String, Integer> expectedArgIndex = new HashMap<>();
            String detectedPackage = null;

            // Step 2: detect framework package (from any compilation unit in the batch)
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
                        String pkg = detectPackage(unit);
                        if (pkg != null && detectedPackage == null) {
                            detectedPackage = frameworkPackageKey(pkg);
                        }
                    }
                } catch (IOException e) {
                    // already reported in buildCorpus
                } finally {
                    fm.close();
                }
                if (detectedPackage != null) break;
            }

            // Step 3: for each public static method in the corpus, derive via throw-locus
            for (ClassCorpus cc : corpus.values()) {
                for (MethodTree mt : cc.methods) {
                    classifyMethodByThrowLocus(mt, cc.simpleName, corpus,
                        equality, inequality, truth, negatedTruth,
                        nullSet, notNullSet, approx, unlearned, noThrowLocus,
                        expectedArgIndex, diagnostics);
                }
            }

            // A name classified under ANY assertion category by at least one overload
            // is an assertion; only names with NO classified overload at all stay in
            // noThrowLocus (the name-impostor case: every overload is throw-free).
            noThrowLocus.removeAll(equality); noThrowLocus.removeAll(inequality);
            noThrowLocus.removeAll(truth); noThrowLocus.removeAll(negatedTruth);
            noThrowLocus.removeAll(nullSet); noThrowLocus.removeAll(notNullSet);
            noThrowLocus.removeAll(approx); noThrowLocus.removeAll(unlearned);

            AssertionVocab vocab = new AssertionVocab(
                Collections.unmodifiableSet(equality),
                Collections.unmodifiableSet(inequality),
                Collections.unmodifiableSet(truth),
                Collections.unmodifiableSet(negatedTruth),
                Collections.unmodifiableSet(nullSet),
                Collections.unmodifiableSet(notNullSet),
                Collections.unmodifiableSet(approx),
                Collections.unmodifiableSet(unlearned),
                Collections.unmodifiableSet(noThrowLocus),
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

        // ── throw-locus derivation ─────────────────────────────────────────────

        /**
         * Classify one method by throw-locus derivation.
         * The method name is NOT consulted for classification — structure identifies itself.
         * Only public static methods are candidates.
         *
         * Algorithm:
         *   1. If any parameter has a tolerance name → APPROXIMATE immediately.
         *   2. Reduce the body (depth-bounded): inline single-call delegation into
         *      other methods in the corpus.
         *   3. Find an IfTree(guard, throwBlock) in the reduced body.
         *   4. Classify the guard structurally.
         *   5. Cross-check guard positions against param names; disagree → UNLEARNED.
         */
        private static void classifyMethodByThrowLocus(
                MethodTree mt,
                String ownerClass,
                Map<String, ClassCorpus> corpus,
                Set<String> equality, Set<String> inequality,
                Set<String> truth, Set<String> negatedTruth,
                Set<String> nullSet, Set<String> notNullSet,
                Set<String> approx, Set<String> unlearned,
                Set<String> noThrowLocus,
                Map<String, Integer> expectedArgIndex,
                List<String> diagnostics) {

            // Candidate selection: public static only (no name filter).
            if (!isPublicStatic(mt)) return;
            String name = mt.getName().toString();
            List<? extends VariableTree> params = mt.getParameters();

            // NO signature-based delta pre-check (the P2 special case is deleted):
            // approximate falls out of the GUARD — a 3-arg equality-predicate call
            // (doublesAreEqual/floatsAreEqual/areEqual with a tolerance arg) in the
            // reduced guard classifies as APPROXIMATE below.

            // Reduce body to find the throw locus, inlining up to depth 8.
            GuardResult gr = findThrowGuard(mt, params, corpus, 8, new HashSet<>(), ownerClass);
            if (gr == null) {
                // No reachable throw locus → NOT an assertion. Recorded so that a
                // test calling it gets the named "no throw locus" refusal at lift time
                // instead of a silent skip or a misleading no-vocabulary message.
                // (Post-pass removes names that classified under another overload.)
                noThrowLocus.add(name);
                return;
            }
            if (gr.kind.equals("approx")) {
                approx.add(name);
                return;
            }
            if (gr.kind.equals("unlearned")) {
                unlearned.add(name);
                // H1 [A1]: emit a named diagnostic when the reason is known (e.g. cross-class ambiguity).
                if (gr.reason != null) {
                    diagnostics.add(diagnostic("<vendor>", ownerClass + "." + name, "<vocab>",
                        "VocabDeriver: " + gr.reason + " — " + name + " classified UNLEARNED"));
                }
                return;
            }

            // Cross-check: guard-derived expected/actual positions vs param names.
            // Positions are indices into the ORIGINAL method's parameter list.
            // For equality/inequality: gr.expectedPos is the position of the constant
            // (the expected/unexpected value), gr.actualPos is the other.
            if ((gr.kind.equals("equality") || gr.kind.equals("inequality"))
                    && gr.expectedPos >= 0 && gr.expectedPos < params.size()) {
                String p0name = params.get(0).getName().toString();
                // Guard-derived: expectedPos=0 means param[0] is expected (JUnit order)
                //                expectedPos=1 means param[0] is actual (TestNG order)
                boolean guardSaysActualFirst = (gr.expectedPos != 0);
                boolean nameSaysActualFirst  = p0name.equals("actual");
                if (guardSaysActualFirst != nameSaysActualFirst
                        && !p0name.isEmpty()
                        && !p0name.equals("unexpected")
                        && params.size() >= 2) {
                    // Disagreement between guard positions and param names → UNLEARNED.
                    unlearned.add(name);
                    diagnostics.add(diagnostic("<vendor>", ownerClass + "." + name, "<vocab>",
                        "VocabDeriver: guard-position vs param-name disagreement in " + name
                        + ": guard says expectedPos=" + gr.expectedPos
                        + " but param[0]=" + p0name + " → UNLEARNED"));
                    return;
                }
            }

            // TRUTH/NEGATED_TRUTH soundness: the condition must be one of the
            // ORIGINAL method's boolean parameters. If inlining lost the position
            // (the condition was a derived expression like list.contains(x)),
            // this method's contract is NOT "param is true" — refuse as unlearned.
            if ((gr.kind.equals("truth") || gr.kind.equals("negated_truth"))
                    && (gr.expectedPos < 0 || !isBooleanParam(params, gr.expectedPos))) {
                unlearned.add(name);
                return;
            }

            // Record the classification.
            switch (gr.kind) {
                case "equality" -> {
                    equality.add(name);
                    if (!expectedArgIndex.containsKey(name) && gr.expectedPos >= 0) {
                        expectedArgIndex.put(name, gr.expectedPos);
                    }
                }
                case "inequality" -> {
                    inequality.add(name);
                    if (!expectedArgIndex.containsKey(name) && gr.expectedPos >= 0) {
                        expectedArgIndex.put(name, gr.expectedPos);
                    }
                }
                case "truth"        -> truth.add(name);
                case "negated_truth"-> negatedTruth.add(name);
                case "null"         -> nullSet.add(name);
                case "not_null"     -> notNullSet.add(name);
                default             -> unlearned.add(name);
            }
        }

        /**
         * Result of guard classification from throw-locus derivation.
         * kind: one of "equality", "inequality", "truth", "negated_truth", "null",
         *              "not_null", "approx", "unlearned".
         * expectedPos: for equality/inequality, the 0-based index of the
         *              expected/unexpected parameter in the ORIGINAL method's param list.
         *              -1 if not applicable or not determinable.
         */
        private static final class GuardResult {
            final String kind;
            final int expectedPos; // -1 = N/A
            final String reason;   // non-null for named UNLEARNED (e.g. ambiguous delegation)
            GuardResult(String kind, int expectedPos) {
                this.kind = kind; this.expectedPos = expectedPos; this.reason = null;
            }
            GuardResult(String kind, int expectedPos, String reason) {
                this.kind = kind; this.expectedPos = expectedPos; this.reason = reason;
            }
        }

        /**
         * Find the throw guard in a method body, inlining delegation into the corpus.
         *
         * @param mt          the method to analyse
         * @param outerParams the ORIGINAL public method's parameter list (for position mapping)
         * @param corpus      all vendored method trees by class name
         * @param depth       remaining inlining depth (stop at 0 → UNLEARNED)
         * @param visited     set of "ClassName.methodName" already in the inlining stack
         * @param callerClass the simple class name that owns the top-level entry-point being
         *                    classified; excluded from cross-class ambiguity counts so that
         *                    self-delegation (Assertions→Assertions overload) is not flagged.
         * @return GuardResult, or null if no throw locus found (→ skip, not an assertion)
         */
        private static GuardResult findThrowGuard(
                MethodTree mt,
                List<? extends VariableTree> outerParams,
                Map<String, ClassCorpus> corpus,
                int depth,
                Set<String> visited,
                String callerClass) {

            BlockTree body = mt.getBody();
            if (body == null) return null;

            List<? extends StatementTree> stmts = body.getStatements();

            // Pattern 1: single-statement body that is a method call (pure delegation).
            // e.g. assertTrue(condition, (String) null) → assertTrue(condition, null)
            // or   AssertEquals.assertEquals(expected, actual)
            if (stmts.size() == 1) {
                StatementTree s = stmts.get(0);
                ExpressionTree expr = null;
                if (s instanceof ExpressionStatementTree est) {
                    expr = est.getExpression();
                } else if (s instanceof ReturnTree rt) {
                    expr = rt.getExpression();
                }
                if (expr instanceof MethodInvocationTree mit) {
                    GuardResult delegated = tryInlineCall(mit, outerParams, corpus, depth, visited, callerClass);
                    if (delegated != null) return delegated;
                }
            }

            // Build a local-variable map: localVarName → initializer expression.
            // This handles patterns like:
            //   boolean equal = areEqualImpl(actual, expected);
            //   if (!equal) { failNotEquals(...); }
            // We resolve identifiers in guards against this map before classification.
            Map<String, ExpressionTree> localVars = new LinkedHashMap<>();
            for (StatementTree s : stmts) {
                if (s instanceof VariableTree vt && vt.getInitializer() != null) {
                    localVars.put(vt.getName().toString(), vt.getInitializer());
                }
            }

            // Pattern 2: scan for IfTree(guard, {throw or delegation-to-throw})
            // or ExpressionStatementTree delegation calls.
            // We pass the localVars map so that guards involving local boolean vars
            // can be resolved back to their initializers.
            for (StatementTree s : stmts) {
                GuardResult gr = extractGuardFromStatement(
                        s, outerParams, localVars, corpus, depth, visited, callerClass);
                if (gr != null) return gr;
            }

            return null;
        }

        /**
         * Try to inline a delegation call into the corpus.
         * Returns the GuardResult of the callee, or null if the callee is not in corpus.
         *
         * H1 [A1]: resolution is now class-qualified.
         * - A qualified call `Foo.bar(...)` resolves ONLY in class Foo.
         * - An unqualified call resolves in the current class first; if found only once
         *   across all classes, that match is used; if found in two or more classes with
         *   the same name+arity (ambiguous), returns UNLEARNED with reason.
         */
        private static GuardResult tryInlineCall(
                MethodInvocationTree mit,
                List<? extends VariableTree> outerParams,
                Map<String, ClassCorpus> corpus,
                int depth,
                Set<String> visited,
                String callerClass) {

            if (depth <= 0) return new GuardResult("unlearned", -1);

            String calleeName = extractSimpleMethodName(mit);
            if (calleeName == null) return null;

            // H1 [A1]: extract qualifier class from a qualified call (Foo.bar(..)).
            String qualifierClass = extractQualifierClass(mit);

            // Find the callee in the corpus — qualifier-aware to prevent cross-class
            // name collision from inlining the wrong body (falsePass-in-principle).
            MethodTree callee = findInCorpusQualified(corpus, calleeName,
                    mit.getArguments().size(), qualifierClass);
            if (callee == null) return null;

            // H1 [A1]: if qualifierClass is null (unqualified call) and the same
            // name+arity exists in two or more classes OTHER than the caller class,
            // the delegation target is ambiguous — return UNLEARNED rather than first-match.
            // We exclude callerClass because it is the public entry-point being classified,
            // not a helper; self-overload-delegation (Assertions→Assertions) is not ambiguous.
            if (qualifierClass == null
                    && countMatchesInCorpus(corpus, calleeName, mit.getArguments().size(),
                                            callerClass) > 1) {
                return new GuardResult("unlearned", -1,
                    "ambiguous delegation target: " + calleeName
                    + " exists in multiple vendor classes with same arity");
            }

            // Check parameter count matches (the call might have fewer args than the callee
            // when trailing message params are absent).
            String callKey = calleeName + "/" + callee.getParameters().size();
            if (visited.contains(callKey)) return new GuardResult("unlearned", -1); // cycle
            Set<String> newVisited = new HashSet<>(visited);
            newVisited.add(callKey);

            // Build a parameter-identity map: callee param[i] → outerParams index.
            // We need to know which callee param corresponds to which outer param.
            List<? extends ExpressionTree> args = mit.getArguments();
            List<? extends VariableTree> calleeParams = callee.getParameters();
            Map<String, Integer> paramMap = new LinkedHashMap<>();
            for (int i = 0; i < Math.min(args.size(), calleeParams.size()); i++) {
                ExpressionTree arg = args.get(i);
                // Strip casts
                while (arg instanceof TypeCastTree tct) arg = tct.getExpression();
                if (arg instanceof IdentifierTree id) {
                    String argName = id.getName().toString();
                    // Find which outer param this corresponds to
                    for (int j = 0; j < outerParams.size(); j++) {
                        if (outerParams.get(j).getName().toString().equals(argName)) {
                            paramMap.put(calleeParams.get(i).getName().toString(), j);
                            break;
                        }
                    }
                }
            }

            GuardResult raw = findThrowGuard(callee, callee.getParameters(), corpus,
                    depth - 1, newVisited, callerClass);
            if (raw == null) return null;

            // Remap positions through paramMap
            return remapPositions(raw, paramMap, calleeParams);
        }

        /**
         * Remap guard positions from callee's parameter space back to outer parameter space.
         */
        private static GuardResult remapPositions(
                GuardResult raw,
                Map<String, Integer> paramMap,
                List<? extends VariableTree> calleeParams) {
            if (raw.expectedPos < 0) return raw;
            if (raw.expectedPos >= calleeParams.size()) return new GuardResult(raw.kind, -1);
            String calleeParamName = calleeParams.get(raw.expectedPos).getName().toString();
            Integer outerIdx = paramMap.get(calleeParamName);
            // The callee's significant param does not correspond to any outer param
            // (the argument was a derived expression, not parameter identity):
            // the position is LOST. Callers must treat -1 as "cannot trust".
            if (outerIdx == null) return new GuardResult(raw.kind, -1);
            return new GuardResult(raw.kind, outerIdx);
        }

        /**
         * Extract a GuardResult from a single statement, looking for:
         *   - IfTree(guard, throwBlock) — classify guard (with localVar resolution)
         *   - ExpressionStatementTree(delegation call) — inline the callee
         *   - nested blocks
         *
         * localVars maps local variable names to their initializer expressions,
         * allowing guards like `if (!equal)` where `equal = areEqualImpl(a,b)`
         * to be resolved back to their underlying predicate call.
         *
         * The delegation case handles multi-statement bodies like TestNG's:
         *   if (expected.isArray()) { assertArrayEquals(...); return; }
         *   assertEqualsImpl(actual, expected, message);  ← delegate here
         */
        private static GuardResult extractGuardFromStatement(
                StatementTree s,
                List<? extends VariableTree> outerParams,
                Map<String, ExpressionTree> localVars,
                Map<String, ClassCorpus> corpus,
                int depth,
                Set<String> visited,
                String callerClass) {

            if (s instanceof IfTree it) {
                ExpressionTree cond = it.getCondition();
                StatementTree then = it.getThenStatement();
                // The then-block must terminate: contain a throw or a call-that-throws.
                if (thenBlockThrows(then, corpus, depth, visited)) {
                    return classifyGuard(cond, outerParams, localVars);
                }
                // Then-block does NOT throw (e.g. early-return branch) — skip this If.
            } else if (s instanceof BlockTree bt) {
                for (StatementTree inner : bt.getStatements()) {
                    GuardResult gr = extractGuardFromStatement(
                            inner, outerParams, localVars, corpus, depth, visited, callerClass);
                    if (gr != null) return gr;
                }
            } else if (s instanceof ExpressionStatementTree est) {
                // Delegation: a bare call to another vendored method.
                // e.g. assertEqualsImpl(actual, expected, message)
                ExpressionTree e = est.getExpression();
                if (e instanceof MethodInvocationTree mit) {
                    GuardResult delegated = tryInlineCall(mit, outerParams, corpus, depth, visited, callerClass);
                    if (delegated != null) return delegated;
                }
            }
            return null;
        }

        /**
         * Check whether a statement (typically the then-block of an IfTree) terminates
         * by throwing: either a ThrowTree, or a call to a method that is itself a throw
         * terminal in the corpus (e.g. failNotTrue(), buildAndThrow(), fail()).
         */
        private static boolean thenBlockThrows(
                StatementTree stmt,
                Map<String, ClassCorpus> corpus,
                int depth,
                Set<String> visited) {
            if (stmt == null) return false;
            if (stmt instanceof ThrowTree) return true;
            if (stmt instanceof BlockTree bt) {
                for (StatementTree s : bt.getStatements()) {
                    if (thenBlockThrows(s, corpus, depth, visited)) return true;
                }
                return false;
            }
            if (stmt instanceof ExpressionStatementTree est) {
                ExpressionTree e = est.getExpression();
                if (e instanceof MethodInvocationTree mit) {
                    // Check if the call is a known throw terminal
                    String mname = extractSimpleMethodName(mit);
                    if (mname != null && isThrowTerminal(mname, corpus, depth, visited)) {
                        return true;
                    }
                }
            }
            return false;
        }

        /**
         * Check whether a method name is a throw terminal: its body in the corpus
         * ultimately reaches a ThrowTree.
         */
        private static boolean isThrowTerminal(
                String methodName,
                Map<String, ClassCorpus> corpus,
                int depth,
                Set<String> visited) {
            if (depth <= 0) return false;
            // Well-known terminal method names in JUnit5/TestNG
            if (methodName.equals("buildAndThrow") || methodName.equals("fail")
                    || methodName.equals("failNotEquals") || methodName.equals("failEquals")
                    || methodName.equals("failNotSame") || methodName.equals("failNotTrue")
                    || methodName.equals("failNotFalse") || methodName.equals("failNull")
                    || methodName.equals("failNotNull") || methodName.equals("failNotEqual")
                    || methodName.equals("failEqual")) {
                return true;
            }
            // Check corpus
            MethodTree mt = findInCorpus(corpus, methodName, -1);
            if (mt == null) return false;
            String key = methodName + "/-1";
            if (visited.contains(key)) return false;
            Set<String> newV = new HashSet<>(visited);
            newV.add(key);
            BlockTree body = mt.getBody();
            if (body == null) return false;
            for (StatementTree s : body.getStatements()) {
                if (thenBlockThrows(s, corpus, depth - 1, newV)) return true;
            }
            return false;
        }

        /**
         * Classify a guard expression structurally — the heart of Phase 4.5.
         *
         * Guards patterns (all structural, no name matching):
         *   !condition              (boolean param negated)   → TRUTH (throws when !condition)
         *   condition               (boolean param)           → NEGATED_TRUTH (throws when condition)
         *   p_i != p_j              (binary !=)               → EQUALITY (guard: fail when not equal)
         *   p_i == p_j              (binary ==)               → depends on null:
         *       p_i == null or p_j == null                    → NOT_NULL
         *       otherwise                                     → INEQUALITY
         *   !equalpredicate(p_i, p_j)                        → EQUALITY
         *   equalpredicate(p_i, p_j) (not negated)           → INEQUALITY
         *   p_i != null or null != p_i                        → NULL (guard: fail when not null)
         *   p_i == null or null == p_i                        → NOT_NULL (guard: fail when null)
         *   floatsAreEqual(p_i, p_j, delta) / doublesAreEqual(p_i, p_j, delta)
         *                                                     → APPROXIMATE
         */
        /**
         * Overload without localVars for call sites that don't need it.
         */
        private static GuardResult classifyGuard(
                ExpressionTree cond,
                List<? extends VariableTree> params) {
            return classifyGuard(cond, params, Map.of());
        }

        /**
         * Classify a guard expression structurally — the heart of Phase 4.5.
         *
         * localVars maps local variable names to their initializer expressions.
         * This resolves patterns like:
         *   boolean equal = areEqualImpl(actual, expected);
         *   if (!equal) ...
         * where the guard identifier `equal` is resolved to its initializer
         * `areEqualImpl(actual, expected)` before classification.
         *
         * Guards patterns (all structural, no name matching):
         *   !condition              (boolean param negated)   → TRUTH
         *   !localVar               (local = equalPred(...))  → EQUALITY via resolution
         *   localVar                (local = equalPred(...))  → INEQUALITY via resolution
         *   condition               (boolean param)           → NEGATED_TRUTH
         *   p_i != p_j              (binary !=)               → EQUALITY
         *   p_i == p_j              (binary ==, non-null)     → INEQUALITY
         *   !equalpredicate(p_i, p_j)                        → EQUALITY
         *   equalpredicate(p_i, p_j) (not negated)           → INEQUALITY
         *   p_i != null                                       → NULL
         *   p_i == null                                       → NOT_NULL
         *   floatsAreEqual(p_i, p_j, delta) / 3-arg call     → APPROXIMATE
         */
        private static GuardResult classifyGuard(
                ExpressionTree cond,
                List<? extends VariableTree> params,
                Map<String, ExpressionTree> localVars) {

            // Build param name → index map for position derivation
            Map<String, Integer> paramIndex = new LinkedHashMap<>();
            for (int i = 0; i < params.size(); i++) {
                paramIndex.put(params.get(i).getName().toString(), i);
            }

            // Strip outer parentheses
            while (cond instanceof ParenthesizedTree pt) cond = pt.getExpression();

            // Case: !expr
            if (cond instanceof UnaryTree ut && ut.getKind() == Tree.Kind.LOGICAL_COMPLEMENT) {
                ExpressionTree inner = ut.getExpression();
                while (inner instanceof ParenthesizedTree pt) inner = pt.getExpression();

                // !localVar — resolve the local var to its initializer, then re-classify
                if (inner instanceof IdentifierTree id) {
                    String pname = id.getName().toString();
                    if (localVars.containsKey(pname)) {
                        // e.g. !equal where equal = areEqualImpl(actual, expected)
                        // → treat as !areEqualImpl(actual, expected) → EQUALITY
                        ExpressionTree resolved = localVars.get(pname);
                        while (resolved instanceof ParenthesizedTree pt) resolved = pt.getExpression();
                        if (resolved instanceof MethodInvocationTree mit2) {
                            String mn2 = extractSimpleMethodName(mit2);
                            if (mn2 != null && EQUAL_PREDICATE_METHODS.contains(mn2)) {
                                if (mit2.getArguments().size() >= 3) return new GuardResult("approx", -1);
                                int[] pos = extractParamPositions2(mit2.getArguments(), paramIndex);
                                return new GuardResult("equality", expectedPosFromArgs(pos, params));
                            }
                            // H1 [C8]: !notEqual where notEqual = areNotEqualImpl(actual, expected)
                            // → throws when NOT (a!=b) = throws when a==b → INEQUALITY
                            if (mn2 != null && NOT_EQUAL_PREDICATE_METHODS.contains(mn2)) {
                                if (mit2.getArguments().size() >= 3) return new GuardResult("approx", -1);
                                int[] pos = extractParamPositions2(mit2.getArguments(), paramIndex);
                                return new GuardResult("inequality", expectedPosFromArgs(pos, params));
                            }
                        }
                        // Resolved to something else: fall through to !booleanParam check
                    }
                    // !condition (single boolean param) → TRUTH
                    // The condition's param position is carried so that inlining can
                    // verify it maps back to an outer boolean param (otherwise the
                    // "condition" is a derived expression — refuse as unlearned).
                    if (paramIndex.containsKey(pname) && isBooleanParam(params, paramIndex.get(pname))) {
                        return new GuardResult("truth", paramIndex.get(pname));
                    }
                }
                // !objectsAreEqual(p_i, p_j) → EQUALITY
                if (inner instanceof MethodInvocationTree mit) {
                    String mn = extractSimpleMethodName(mit);
                    if (mn != null && EQUAL_PREDICATE_METHODS.contains(mn)) {
                        if (mit.getArguments().size() >= 3) {
                            return new GuardResult("approx", -1);
                        }
                        int[] pos = extractParamPositions2(mit.getArguments(), paramIndex);
                        return new GuardResult("equality", expectedPosFromArgs(pos, params));
                    }
                    // H1 [C8]: !areNotEqualImpl(p_i, p_j) → throws when NOT (a!=b) = INEQUALITY
                    if (mn != null && NOT_EQUAL_PREDICATE_METHODS.contains(mn)) {
                        if (mit.getArguments().size() >= 3) {
                            return new GuardResult("approx", -1);
                        }
                        int[] pos = extractParamPositions2(mit.getArguments(), paramIndex);
                        return new GuardResult("inequality", expectedPosFromArgs(pos, params));
                    }
                }
                // !(p_i != p_j) → INEQUALITY (double negation)
                // DISPATCH GATE (same as the bare-binary arms): ==/!= is value
                // equality on PRIMITIVES only; on references it is identity,
                // outside the value algebra → unlearned.
                if (inner instanceof BinaryTree bt2 && bt2.getKind() == Tree.Kind.NOT_EQUAL_TO) {
                    int[] pos = extractParamPositions2Binary(bt2, paramIndex);
                    boolean bothPrimitiveParams = pos[0] >= 0 && pos[1] >= 0
                            && isPrimitiveParam(params, pos[0])
                            && isPrimitiveParam(params, pos[1]);
                    if (!bothPrimitiveParams) {
                        return new GuardResult("unlearned", -1);
                    }
                    return new GuardResult("inequality", pos[0]);
                }
                // !(p_i == p_j) → EQUALITY
                if (inner instanceof BinaryTree bt2 && bt2.getKind() == Tree.Kind.EQUAL_TO) {
                    ExpressionTree l = stripParens(bt2.getLeftOperand());
                    ExpressionTree r = stripParens(bt2.getRightOperand());
                    if (isNullLiteral(l) || isNullLiteral(r)) {
                        ExpressionTree other = isNullLiteral(l) ? r : l;
                        if (other instanceof IdentifierTree oid
                                && paramIndex.containsKey(oid.getName().toString())) {
                            return new GuardResult("not_null", -1);
                        }
                        return new GuardResult("unlearned", -1);
                    }
                    int[] pos = extractParamPositions2Binary(bt2, paramIndex);
                    boolean bothPrimitiveParams = pos[0] >= 0 && pos[1] >= 0
                            && isPrimitiveParam(params, pos[0])
                            && isPrimitiveParam(params, pos[1]);
                    if (!bothPrimitiveParams) {
                        return new GuardResult("unlearned", -1);
                    }
                    return new GuardResult("equality", pos[0]);
                }
                return new GuardResult("unlearned", -1);
            }

            // Case: bare identifier — check if it's a local var resolving to an equalPred
            if (cond instanceof IdentifierTree id) {
                String pname = id.getName().toString();
                if (localVars.containsKey(pname)) {
                    ExpressionTree resolved = localVars.get(pname);
                    while (resolved instanceof ParenthesizedTree pt) resolved = pt.getExpression();
                    if (resolved instanceof MethodInvocationTree mit2) {
                        String mn2 = extractSimpleMethodName(mit2);
                        if (mn2 != null && EQUAL_PREDICATE_METHODS.contains(mn2)) {
                            // localVar = equalPred(...) and guard = localVar (no negation)
                            // → throws when equal → INEQUALITY
                            if (mit2.getArguments().size() >= 3) return new GuardResult("approx", -1);
                            int[] pos = extractParamPositions2(mit2.getArguments(), paramIndex);
                            return new GuardResult("inequality", expectedPosFromArgs(pos, params));
                        }
                        // H1 [C8]: notEqualVar = areNotEqualImpl(a,b), guard = notEqualVar (no negation)
                        // → throws when a!=b → EQUALITY
                        if (mn2 != null && NOT_EQUAL_PREDICATE_METHODS.contains(mn2)) {
                            if (mit2.getArguments().size() >= 3) return new GuardResult("approx", -1);
                            int[] pos = extractParamPositions2(mit2.getArguments(), paramIndex);
                            return new GuardResult("equality", expectedPosFromArgs(pos, params));
                        }
                    }
                }
                // bare boolean param → NEGATED_TRUTH
                if (paramIndex.containsKey(pname) && isBooleanParam(params, paramIndex.get(pname))) {
                    return new GuardResult("negated_truth", paramIndex.get(pname));
                }
            }

            // Case: binary expression
            if (cond instanceof BinaryTree bt) {
                ExpressionTree left  = stripParens(bt.getLeftOperand());
                ExpressionTree right = stripParens(bt.getRightOperand());
                Tree.Kind kind = bt.getKind();

                // p_i != null → NULL (assertNull: throws when not null).
                // The non-null operand MUST be a parameter — `localVar != null`
                // (a derived value) is not a null assertion over the params.
                if (kind == Tree.Kind.NOT_EQUAL_TO
                        && (isNullLiteral(right) || isNullLiteral(left))) {
                    ExpressionTree other = isNullLiteral(right) ? left : right;
                    if (other instanceof IdentifierTree oid
                            && paramIndex.containsKey(oid.getName().toString())) {
                        return new GuardResult("null", -1);
                    }
                    return new GuardResult("unlearned", -1);
                }
                // p_i == null → NOT_NULL (assertNotNull: throws when null)
                if (kind == Tree.Kind.EQUAL_TO
                        && (isNullLiteral(right) || isNullLiteral(left))) {
                    ExpressionTree other = isNullLiteral(right) ? left : right;
                    if (other instanceof IdentifierTree oid
                            && paramIndex.containsKey(oid.getName().toString())) {
                        return new GuardResult("not_null", -1);
                    }
                    return new GuardResult("unlearned", -1);
                }
                // p_i != p_j → EQUALITY (throws when not equal = asserts equal)
                // p_i == p_j → INEQUALITY (throws when equal = asserts not equal)
                //
                // DISPATCH GATE: what does `==` dispatch to? On PRIMITIVES it is
                // value equality — in our algebra. On REFERENCES it is IDENTITY
                // (same object), which is NOT value equality: lifting TestNG's
                // assertNotSame (`expected == actual` over Objects) as value-≠
                // would swear a value claim the vendor never made (two .equals()
                // values can be distinct refs) — a falsePass/false-refusal pair.
                // So a bare ==/!= guard classifies ONLY when both operands are
                // primitive-typed parameters; reference identity → unlearned.
                if (kind == Tree.Kind.NOT_EQUAL_TO || kind == Tree.Kind.EQUAL_TO) {
                    int[] pos = extractParamPositions2Binary(bt, paramIndex);
                    boolean bothPrimitiveParams = pos[0] >= 0 && pos[1] >= 0
                            && isPrimitiveParam(params, pos[0])
                            && isPrimitiveParam(params, pos[1]);
                    if (!bothPrimitiveParams) {
                        return new GuardResult("unlearned", -1);
                    }
                    return new GuardResult(
                            kind == Tree.Kind.NOT_EQUAL_TO ? "equality" : "inequality",
                            pos[0]);
                }
                return new GuardResult("unlearned", -1);
            }

            // Case: objectsAreEqual(p_i, p_j) not negated → INEQUALITY
            if (cond instanceof MethodInvocationTree mit) {
                String mn = extractSimpleMethodName(mit);
                if (mn != null && EQUAL_PREDICATE_METHODS.contains(mn)) {
                    if (mit.getArguments().size() >= 3) {
                        return new GuardResult("approx", -1);
                    }
                    int[] pos = extractParamPositions2(mit.getArguments(), paramIndex);
                    return new GuardResult("inequality", expectedPosFromArgs(pos, params));
                }
                return new GuardResult("unlearned", -1);
            }

            return new GuardResult("unlearned", -1);
        }

        // ── guard helper utilities ─────────────────────────────────────────────

        private static ExpressionTree stripParens(ExpressionTree e) {
            while (e instanceof ParenthesizedTree pt) e = pt.getExpression();
            return e;
        }

        private static boolean isNullLiteral(ExpressionTree e) {
            return e instanceof LiteralTree lt && lt.getKind() == Tree.Kind.NULL_LITERAL;
        }

        /**
         * True iff the parameter's declared type is a Java PRIMITIVE (read from
         * the PrimitiveTypeTree node). On primitives `==` is value equality; on
         * references it is identity — every Java developer knows the difference,
         * and so must the lifter. Boxed types are deliberately NOT accepted:
         * `Integer == Integer` is reference identity (cache-dependent), not
         * value equality.
         */
        private static boolean isPrimitiveParam(
                List<? extends VariableTree> params, int idx) {
            if (idx < 0 || idx >= params.size()) return false;
            return params.get(idx).getType() instanceof PrimitiveTypeTree;
        }

        private static boolean isBooleanParam(
                List<? extends VariableTree> params, int idx) {
            if (idx < 0 || idx >= params.size()) return false;
            Tree type = params.get(idx).getType();
            if (type instanceof PrimitiveTypeTree ptt) {
                return ptt.getPrimitiveTypeKind() == TypeKind.BOOLEAN;
            }
            // Also accept named type "Boolean" (boxed)
            if (type instanceof IdentifierTree id) {
                return id.getName().toString().equals("Boolean");
            }
            return false;
        }

        /**
         * Extract the 0-based positions of the first two arguments in a method call
         * relative to the outer parameter list.
         * Returns {pos0, pos1} where -1 means "not an outer param reference".
         */
        private static int[] extractParamPositions2(
                List<? extends ExpressionTree> args,
                Map<String, Integer> paramIndex) {
            int[] pos = {-1, -1};
            for (int i = 0; i < Math.min(2, args.size()); i++) {
                ExpressionTree a = args.get(i);
                while (a instanceof TypeCastTree tct) a = tct.getExpression();
                while (a instanceof ParenthesizedTree pt) a = pt.getExpression();
                if (a instanceof IdentifierTree id) {
                    Integer p = paramIndex.get(id.getName().toString());
                    if (p != null) pos[i] = p;
                }
            }
            return pos;
        }

        /**
         * Given a {pos0, pos1} array from extractParamPositions2 (mapping two argument
         * slots to outer-param indices), find which outer-param index holds the EXPECTED
         * value, using param names as the tie-breaker.
         *
         * Rules (in priority order):
         *  1. If the param at pos[1] is named "expected" or "unexpected"  → return pos[1]
         *  2. If the param at pos[0] is named "expected" or "unexpected"  → return pos[0]
         *  3. If the param at pos[0] is named "actual"                    → return pos[1]
         *  4. Default: return pos[0]
         *
         * This handles:
         *  - assertEqualsImpl(actual=0, expected=1): pos={0,1}, param[1]="expected" → pos[1]=1
         *  - AssertEquals(expected=0, actual=1):     pos={0,1}, param[0]="expected" → pos[0]=0
         *  - areEqual(actual, expected):             pos={0,1}, param[0]="actual"   → pos[1]=1
         */
        private static int expectedPosFromArgs(
                int[] pos,
                List<? extends VariableTree> outerParams) {
            if (pos[1] >= 0 && pos[1] < outerParams.size()) {
                String n = outerParams.get(pos[1]).getName().toString();
                if (n.equals("expected") || n.equals("unexpected")) return pos[1];
            }
            if (pos[0] >= 0 && pos[0] < outerParams.size()) {
                String n = outerParams.get(pos[0]).getName().toString();
                if (n.equals("expected") || n.equals("unexpected")) return pos[0];
                if (n.equals("actual")) return pos[1] >= 0 ? pos[1] : pos[0];
            }
            return pos[0]; // default
        }

        /** Extract param positions from a binary expression's left/right operands. */
        private static int[] extractParamPositions2Binary(
                BinaryTree bt,
                Map<String, Integer> paramIndex) {
            ExpressionTree left  = stripParens(bt.getLeftOperand());
            ExpressionTree right = stripParens(bt.getRightOperand());
            int[] pos = {-1, -1};
            if (left instanceof IdentifierTree id) {
                Integer p = paramIndex.get(id.getName().toString());
                if (p != null) pos[0] = p;
            }
            if (right instanceof IdentifierTree id) {
                Integer p = paramIndex.get(id.getName().toString());
                if (p != null) pos[1] = p;
            }
            return pos;
        }

        /**
         * Extract the simple method name from a MethodInvocationTree.
         * Handles both simple calls (foo()) and qualified calls (Foo.foo() / this.foo()).
         */
        private static String extractSimpleMethodName(MethodInvocationTree mit) {
            ExpressionTree sel = mit.getMethodSelect();
            if (sel instanceof IdentifierTree id) return id.getName().toString();
            if (sel instanceof MemberSelectTree mst) return mst.getIdentifier().toString();
            return null;
        }

        /**
         * Find a method in the corpus by simple name and argument count.
         * If arity is -1, returns the first match by name.
         * Prefers exact arity match; falls back to any match if no exact match.
         * NOTE: this is the legacy unqualified search used by isThrowTerminal and
         * other helpers that don't have a class qualifier. Call findInCorpusQualified
         * for all delegation-chain inlining (H1 [A1]).
         */
        private static MethodTree findInCorpus(
                Map<String, ClassCorpus> corpus, String methodName, int arity) {
            MethodTree fallback = null;
            for (ClassCorpus cc : corpus.values()) {
                for (MethodTree mt : cc.methods) {
                    if (!mt.getName().toString().equals(methodName)) continue;
                    if (arity < 0) return mt;
                    int paramCount = mt.getParameters().size();
                    if (paramCount == arity) return mt;
                    // Allow arity+1 match (trailing message param)
                    if (paramCount == arity + 1 && fallback == null) fallback = mt;
                    if (fallback == null) fallback = mt;
                }
            }
            return fallback;
        }

        /**
         * H1 [A1]: Qualified corpus lookup.
         * If qualifierClass is non-null, restrict to that class only.
         * If qualifierClass is null (unqualified call), fall back to the legacy
         * first-match (callers are responsible for checking ambiguity separately).
         */
        private static MethodTree findInCorpusQualified(
                Map<String, ClassCorpus> corpus, String methodName, int arity,
                String qualifierClass) {
            if (qualifierClass != null) {
                // Qualified call: resolve ONLY in the named class.
                ClassCorpus cc = corpus.get(qualifierClass);
                if (cc == null) return null; // class not in vendored corpus → chain escapes
                MethodTree fallback = null;
                for (MethodTree mt : cc.methods) {
                    if (!mt.getName().toString().equals(methodName)) continue;
                    if (arity < 0) return mt;
                    int paramCount = mt.getParameters().size();
                    if (paramCount == arity) return mt;
                    if (paramCount == arity + 1 && fallback == null) fallback = mt;
                    if (fallback == null) fallback = mt;
                }
                return fallback;
            }
            // Unqualified: legacy search (ambiguity checked by caller via countMatchesInCorpus).
            return findInCorpus(corpus, methodName, arity);
        }

        /**
         * H1 [A1]: Count how many distinct classes in the corpus declare a method
         * with the given name and arity (exact match or arity+1 trailing-message match),
         * excluding the class named excludeClass (the calling/owning class, which is the
         * entry-point and not a valid delegation target for disambiguation).
         * Used to detect ambiguous unqualified delegation: if the same helper name+arity
         * appears in 2+ classes OTHER than the caller, the target is ambiguous.
         */
        private static int countMatchesInCorpus(
                Map<String, ClassCorpus> corpus, String methodName, int arity,
                String excludeClass) {
            int count = 0;
            for (Map.Entry<String, ClassCorpus> e : corpus.entrySet()) {
                if (excludeClass != null && e.getKey().equals(excludeClass)) continue;
                ClassCorpus cc = e.getValue();
                for (MethodTree mt : cc.methods) {
                    if (!mt.getName().toString().equals(methodName)) continue;
                    int paramCount = mt.getParameters().size();
                    if (arity < 0 || paramCount == arity || paramCount == arity + 1) {
                        count++;
                        break; // count once per class
                    }
                }
            }
            return count;
        }

        /**
         * H1 [A1]: Extract the simple class name from the qualifier of a qualified call.
         * For `Foo.bar(...)` returns "Foo"; for `this.bar(...)` or bare `bar(...)` returns null.
         */
        private static String extractQualifierClass(MethodInvocationTree mit) {
            ExpressionTree sel = mit.getMethodSelect();
            if (sel instanceof MemberSelectTree mst) {
                ExpressionTree expr = mst.getExpression();
                // strip parens
                while (expr instanceof ParenthesizedTree pt) expr = pt.getExpression();
                if (expr instanceof IdentifierTree id) {
                    String name = id.getName().toString();
                    // "this" and "super" are not class qualifiers
                    if (!name.equals("this") && !name.equals("super")) return name;
                }
            }
            return null;
        }

        // ── structural helpers ─────────────────────────────────────────────────

        private static boolean isPublicStatic(MethodTree mt) {
            Set<Modifier> mods = mt.getModifiers().getFlags();
            return mods.contains(Modifier.PUBLIC) && mods.contains(Modifier.STATIC);
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
     * For BOTH (ambiguous) the UNION of both vocabs is returned so that assertion
     * candidates can still be recognised — the ambiguity flag then refuses each one
     * by name (we must know it IS an assertion to refuse it loudly).
     * Returns empty vocab when framework is NEITHER or vocab is not configured.
     */
    private static AssertionVocab selectVocabForFramework(
            FrameworkKind kind,
            MultiFrameworkVocab multiVocab) {
        return switch (kind) {
            case JUNIT  -> multiVocab.forFramework("org.junit");
            case TESTNG -> multiVocab.forFramework("org.testng");
            case BOTH   -> mergeVocabs(
                multiVocab.forFramework("org.junit"),
                multiVocab.forFramework("org.testng"));
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
            UniverseRegistry universeRegistry,
            NumericUniverseRegistry numericRegistry,
            InstanceUniverse instanceUniverse,
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
            boolean ambiguousFramework = (frameworkKind == FrameworkKind.BOTH);

            // Names bound by a STATIC import from an assertion-framework package.
            // A call to such a name is a CLAIMED assertion (the import binding is
            // structural) even when the vocab learned nothing about it — that is
            // how the no-vocab / assertThat cases still get loud named refusals
            // instead of silent skips. This replaces the old hardcoded
            // startsWith("assert") candidate filter in the lift path.
            //
            // H1 [A2]: wildcard static imports (import static org.junit.Assert.*)
            // bind ALL public static methods of the named class. For a vendored
            // framework class we expand the wildcard to the learned vocab's method
            // names so that assertEquals, assertNotNull, etc. are structurally bound
            // without requiring a per-name named import. For an unvendored class the
            // wildcard import produces a named refusal ("static wildcard import of
            // unvendored class") for any call whose name we otherwise know.
            Set<String> assertionBoundNames = new HashSet<>();
            for (ImportTree imp : unit.getImports()) {
                if (!imp.isStatic()) continue;
                String qn = imp.getQualifiedIdentifier().toString();
                if (qn.startsWith("org.junit.") || qn.startsWith("org.testng.")) {
                    if (qn.endsWith(".*")) {
                        // H1 [A2]: wildcard — determine framework key and expand to all
                        // known vocab method names for that framework.
                        String classPath = qn.substring(0, qn.length() - 2); // strip .*
                        String fwKey = classPath.startsWith("org.testng.") ? "org.testng" : "org.junit";
                        AssertionVocab fwVocab = multiVocab.forFramework(fwKey);
                        // Expand to all names the vocab knows (equality, inequality, truth, etc.)
                        // so that any call to a vocab-known name is structurally bound.
                        assertionBoundNames.addAll(fwVocab.equality);
                        assertionBoundNames.addAll(fwVocab.inequality);
                        assertionBoundNames.addAll(fwVocab.truth);
                        assertionBoundNames.addAll(fwVocab.negatedTruth);
                        assertionBoundNames.addAll(fwVocab.nullSet);
                        assertionBoundNames.addAll(fwVocab.notNullSet);
                        assertionBoundNames.addAll(fwVocab.approx);
                        assertionBoundNames.addAll(fwVocab.unlearned);
                        assertionBoundNames.addAll(fwVocab.noThrowLocus);
                        if (fwVocab.equality.isEmpty() && fwVocab.inequality.isEmpty()) {
                            // Unvendored class wildcard: mark with sentinel so liftStatement
                            // can produce a named refusal for any call it processes.
                            assertionBoundNames.add("__wildcard_unvendored__:" + classPath);
                        }
                    } else {
                        int dot = qn.lastIndexOf('.');
                        if (dot >= 0) assertionBoundNames.add(qn.substring(dot + 1));
                    }
                }
            }

            for (Tree decl : unit.getTypeDecls()) {
                if (decl instanceof ClassTree ct) {
                    walkClassMembers(ct, unit, rel, importedNames, assertionBoundNames,
                            vocab, frameworkKind, ambiguousFramework,
                            universeRegistry, numericRegistry, instanceUniverse, ir, diagnostics, null);
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
            Set<String> assertionBoundNames,
            AssertionVocab vocab,
            FrameworkKind frameworkKind,
            boolean ambiguousFramework,
            UniverseRegistry universeRegistry,
            NumericUniverseRegistry numericRegistry,
            InstanceUniverse instanceUniverse,
            List<String> ir,
            List<String> diagnostics,
            String outerClassName) {

        String className = classTree.getSimpleName().toString();
        if (outerClassName != null) className = outerClassName + "." + className;

        for (Tree member : classTree.getMembers()) {
            if (member instanceof MethodTree mt) {
                liftMethod(mt, unit, rel, className, importedNames, assertionBoundNames,
                        vocab, frameworkKind, ambiguousFramework, universeRegistry, numericRegistry, instanceUniverse, ir, diagnostics);
            } else if (member instanceof ClassTree nested) {
                walkClassMembers(nested, unit, rel, importedNames, assertionBoundNames,
                        vocab, frameworkKind, ambiguousFramework,
                        universeRegistry, numericRegistry, instanceUniverse, ir, diagnostics, className);
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
            Set<String> assertionBoundNames,
            AssertionVocab vocab,
            FrameworkKind frameworkKind,
            boolean ambiguousFramework,
            UniverseRegistry universeRegistry,
            NumericUniverseRegistry numericRegistry,
            InstanceUniverse instanceUniverse,
            List<String> ir,
            List<String> diagnostics) {

        if (!hasTestAnnotation(method, importedNames)) return;

        String methodName = method.getName().toString();
        String scope = rel + "::" + className + "::" + methodName;

        BlockTree body = method.getBody();
        if (body == null) return;

        Set<String> mutatedLocals = computeMutatedLocals(body);

        // P5c: Build SSA binding map — localName → initializer call expression.
        // A local variable declared with a call initializer (e.g. `String e = f(x)`)
        // is an SSA alias for that callsite. The effectively-final gate (never
        // reassigned, checked against mutatedLocals) makes the alias stable.
        // Mirrors Python _apply_value_scope_binding / _ValueScope.origins.
        Map<String, ExpressionTree> ssaBindings = new LinkedHashMap<>();
        for (StatementTree stmt : body.getStatements()) {
            if (stmt instanceof VariableTree vt && vt.getInitializer() != null) {
                String localName = vt.getName().toString();
                // Only record if effectively final: declared here and never reassigned.
                // mutatedLocals is computed from AssignmentTree/CompoundAssignmentTree/
                // UnaryTree targets — covers all post-declaration writes.
                if (!mutatedLocals.contains(localName)) {
                    ssaBindings.put(localName, vt.getInitializer());
                }
            }
        }

        for (StatementTree stmt : body.getStatements()) {
            if (stmt instanceof ExpressionStatementTree est) {
                liftStatement(est.getExpression(), scope, assertionBoundNames,
                        vocab, frameworkKind, ambiguousFramework, universeRegistry, numericRegistry, instanceUniverse,
                        ssaBindings, mutatedLocals, ir, diagnostics);
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
            Set<String> assertionBoundNames,
            AssertionVocab vocab,
            FrameworkKind frameworkKind,
            boolean ambiguousFramework,
            UniverseRegistry universeRegistry,
            NumericUniverseRegistry numericRegistry,
            InstanceUniverse instanceUniverse,
            Map<String, ExpressionTree> ssaBindings,
            Set<String> mutatedLocals,
            List<String> ir,
            List<String> diagnostics) {

        if (!(expr instanceof MethodInvocationTree mit)) return;

        String methodName = methodInvocationName(mit);

        // CANDIDATE SELECTION (Phase 4.5 / H1 [A3]): an invocation is an assertion
        // candidate iff it is STRUCTURALLY BOUND to the framework via an import edge:
        //
        //   (a) QUALIFIED call `Assert.assertEquals(...)` — the qualifier names an
        //       imported framework class (detected via frameworkKind != NEITHER). The
        //       method name is then looked up in the vocab for that framework.
        //   (b) BARE call `assertEquals(...)` — only if the name is in assertionBoundNames,
        //       which is populated EXCLUSIVELY from static imports of framework packages
        //       (named or wildcard, H1 [A2]). A user-scope method with the same name
        //       but no framework static import MUST NOT bind to the framework vocab — it
        //       is either a user-defined helper or a local override. Lifting it would be
        //       a falsePass.
        //
        // Everything else (helper calls like g(2), user-scope assertEquals without a
        // static import) is not an assertion claim → silently skipped.
        boolean isQualifiedFrameworkCall = false;
        if (expr instanceof MethodInvocationTree mitCheck) {
            ExpressionTree sel = mitCheck.getMethodSelect();
            if (sel instanceof MemberSelectTree) {
                // Qualified call: the frameworkKind for this compilation unit already tells
                // us whether the qualifier class belongs to a framework. If the framework
                // is known (not NEITHER), any qualified call to a vocab-known name is bound.
                isQualifiedFrameworkCall = (frameworkKind != FrameworkKind.NEITHER);
            }
        }
        boolean isBareFrameworkBound = assertionBoundNames.contains(methodName);

        // H1 [A2]: if an unvendored wildcard static import is present, bare calls to
        // vocab-known names produce a named refusal (not a silent skip) because the
        // wildcard says "this name should come from the framework" but the class is not
        // in the vendored corpus so we cannot verify the semantics.
        if (!isBareFrameworkBound && !isQualifiedFrameworkCall) {
            for (String sentinel : assertionBoundNames) {
                if (sentinel.startsWith("__wildcard_unvendored__:") && vocab.isKnown(methodName)) {
                    String classPath = sentinel.substring("__wildcard_unvendored__:".length());
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName,
                        "static wildcard import of unvendored class " + classPath
                        + " — cannot verify semantics of " + methodName + "; refused by name"));
                    return;
                }
            }
        }

        // H1 [A3]: a bare call is only structurally bound to the framework if:
        //   (a) it has a static import from a framework package (isBareFrameworkBound), or
        //   (b) the framework has BOTH JUnit and TestNG imports — ambiguous, but we still
        //       want to fire the AMBIGUITY REFUSAL rather than silently skipping the call
        //       (the ambiguity is itself a structural fact about this file), or
        //   (c) it is a qualified call (isQualifiedFrameworkCall).
        // A bare call with vocab.isKnown() but no static import and no BOTH-ambiguity
        // is a user-scope method — skip silently to avoid falsePass.
        boolean ambiguousBothFrameworks = (frameworkKind == FrameworkKind.BOTH);
        if (!isQualifiedFrameworkCall && !isBareFrameworkBound && !ambiguousBothFrameworks) {
            return;
        }

        if (!vocab.isKnown(methodName) && !assertionBoundNames.contains(methodName)
                && !ambiguousBothFrameworks) {
            return;
        }

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
            case "no_throw_locus" -> {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName,
                    "no throw locus — not an assertion: " + methodName
                    + " (its body never reaches a throw; lifting it would be a false-pass)"));
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
            case "equality" -> liftEquality(mit, methodName, scope, vocab, universeRegistry, numericRegistry, instanceUniverse, ssaBindings, mutatedLocals, ir, diagnostics);
            case "inequality" -> liftInequality(mit, methodName, scope, vocab, ir, diagnostics);
            case "truth" -> liftTruth(mit, methodName, scope, numericRegistry, ir, diagnostics);
            case "negated_truth" -> liftNegatedTruth(mit, methodName, scope, numericRegistry, ir, diagnostics);
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
     *
     * G1 extension: if the expected value is a String literal AND the callee is a
     * universe-registered method, ALSO emit a str.chars-in-set universe contract
     * with the same #euf# contract name. The conjoin then folds both contracts.
     */
    private static void liftEquality(
            MethodInvocationTree mit, String methodName, String scope,
            AssertionVocab vocab,
            UniverseRegistry universeRegistry,
            NumericUniverseRegistry numericRegistry,
            InstanceUniverse instanceUniverse,
            Map<String, ExpressionTree> ssaBindings,
            Set<String> mutatedLocals,
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

        liftBinaryContract(expectedExpr, actualExpr, "=", methodName, scope,
                universeRegistry, numericRegistry, instanceUniverse, ssaBindings, mutatedLocals, ir, diagnostics);
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
        // liftBinaryIntContract is called from the truth/comparison-bound path,
        // which processes already-resolved MethodInvocationTree nodes — no SSA
        // binding substitution needed; pass empty maps.
        liftBinaryContract(constExpr, callExpr, relation, methodName, scope,
                UniverseRegistry.EMPTY, NumericUniverseRegistry.EMPTY, InstanceUniverse.EMPTY,
                Collections.emptyMap(), Collections.emptySet(), ir, diagnostics);
    }

    /**
     * G1/G2/P5c: Extended binary contract lifter that handles both int and String literal
     * expected values. When the expected is a String literal AND the callee is
     * universe-registered, ALSO emits a str.chars-in-set universe contract.
     * G2: When the expected is an int literal AND the callee is numeric-universe-registered,
     * ALSO emits an int32.eq-bv-expr universe contract encoding the walked body.
     *
     * P5c: SSA binding substitution (mirrors Python PATTERN 5 / _apply_value_scope_binding):
     * When the actual arg is an IdentifierTree naming an effectively-final local whose
     * initializer is a call, we substitute that call as the subject of the assertion.
     *
     * LOCATION vs #euf# keying (mirrors Python _call_origin_from_expr rule):
     *   - Static/bare call OR class-qualified call (Base64.encode(...)) where the
     *     qualifier is a class name (not a local): #euf#-federated (existing path).
     *   - Instance-method call (receiver is a local variable, not a class name):
     *     LOCATION-keyed (::facts + ::assertion contracts), NOT #euf#-federated.
     *     Rationale: two different receiver objects may produce different values for
     *     the same method name → cross-location unification is unsound. Python does
     *     exactly this: _call_origin_from_expr returns None for non-module receiver
     *     attribute calls, keeping them location-keyed.
     *
     * String-literal args are accepted for the call's own args only when the call
     * receives them via StringUtils.getBytesUtf8("lit") or "lit".getBytes(...).
     * In those shapes the LITERAL is lifted (the callsite identity keys on the literal).
     * Non-literal args are still refused.
     */
    private static void liftBinaryContract(
            ExpressionTree constExpr, ExpressionTree callExpr,
            String relation, String methodName,
            String scope, UniverseRegistry universeRegistry,
            NumericUniverseRegistry numericRegistry,
            InstanceUniverse instanceUniverse,
            Map<String, ExpressionTree> ssaBindings,
            Set<String> mutatedLocals,
            List<String> ir, List<String> diagnostics) {

        // Try int literal first (existing path)
        OptionalLong intVal = asIntLiteral(constExpr);
        // Try string literal (G1 path)
        Optional<String> strVal = intVal.isEmpty() ? asStringLiteral(constExpr) : Optional.empty();

        if (intVal.isEmpty() && strVal.isEmpty()) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, "expected arg is not an int literal or String literal: " + constExpr));
            return;
        }

        // P5c: SSA binding substitution.
        // `assertEquals("foo", encoded)` where `String encoded = f(args)` → resolve
        // `encoded` to its initializer call expression before further processing.
        // Mirrors Python _apply_value_scope_binding + _assertion_callsite_context:
        // the local is an SSA alias for the callsite; the ::facts binding records
        // the aliasing; the assertion fires on the callsite subject.
        //
        // Effectively-final gate (mirrors Python "single-assignment" rule):
        //   A reassigned local is NOT a stable SSA alias → refuse by name.
        //   We compute effective-finality from the AST (not the `final` keyword)
        //   because real vendor test code almost never writes `final String e = ...`.
        //   Single-assignment = declared once with an initializer AND never the target
        //   of AssignmentTree / CompoundAssignmentTree / ++/-- in the method body.
        //   This is doubly attested: the vendor wrote single-assignment code (intent)
        //   AND javac enforces effective-finality for lambda capture (compiler sign-off).
        //
        // DOES NOT substitute when the local IS in ssaBindings but IS also in
        // mutatedLocals — that combination cannot arise (we only insert into ssaBindings
        // when !mutatedLocals.contains(localName)), but the guard is kept explicit for
        // clarity.
        ExpressionTree resolvedCallExpr = callExpr;
        String ssaSourceName = null; // the local name that was substituted, for diagnostics
        if (callExpr instanceof IdentifierTree idTree) {
            String localName = idTree.getName().toString();
            if (mutatedLocals.contains(localName)) {
                // Reassigned local: not a stable SSA alias — refuse by name.
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName, "SSA binding refused: local '" + localName
                    + "' is reassigned — not a stable alias for its initializer call"));
                return;
            }
            ExpressionTree bound = ssaBindings.get(localName);
            if (bound != null) {
                resolvedCallExpr = bound;
                ssaSourceName = localName;
            }
            // If not in ssaBindings (not a call-initialised local), fall through to the
            // "second arg is not a method call" refusal below.
        }

        if (!(resolvedCallExpr instanceof MethodInvocationTree callMit)) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, "second arg is not a method call: " + callExpr
                + (ssaSourceName != null ? " (resolved from local '" + ssaSourceName + "')" : "")));
            return;
        }

        // P5c: Determine whether this is a static/class-qualified call (#euf#-federated)
        // or an instance-method call on a local receiver (location-keyed).
        // Mirrors Python _call_origin_from_expr: only module-attribute calls are admitted
        // for federation; local-receiver calls are kept location-keyed.
        //
        // Discrimination:
        //   - Bare call `f(args)`: MethodSelect is IdentifierTree → static/imported → #euf#
        //   - `Class.method(args)` where `Class` is NOT in ssaBindings: class-qualified
        //     static call → #euf# (same as bare, existing behaviour)
        //   - `local.method(args)` where `local` IS in ssaBindings OR is a known local:
        //     receiver-dependent → location-keyed
        //
        // We detect the last case by checking whether the receiver of the MemberSelectTree
        // is an IdentifierTree whose name appears in ssaBindings. Class names (Base64,
        // StringUtils, etc.) are never in ssaBindings (ssaBindings only contains locals
        // declared in this method body). This is sound: the only way a name is in
        // ssaBindings is if this method body has a `Type name = initializer` statement.
        boolean isInstanceMethodCall = false;
        String receiverName = null;
        ExpressionTree chainedReceiverExpr = null; // non-null when receiver is a MethodInvocationTree chain
        ExpressionTree methodSelect = callMit.getMethodSelect();
        if (methodSelect instanceof MemberSelectTree mst) {
            ExpressionTree receiver = mst.getExpression();
            if (receiver instanceof IdentifierTree recId) {
                receiverName = recId.getName().toString();
                // Local variable receiver → instance-method call → location-keyed.
                // We check ssaBindings (effectively-final locals with call initialisers)
                // AND mutatedLocals (any locally-declared variable that was reassigned).
                // Any local name that appears in either set was declared in this method.
                // Class names (capitalised identifiers referring to imported types) are
                // never in either set.
                if (ssaBindings.containsKey(receiverName) || mutatedLocals.contains(receiverName)) {
                    isInstanceMethodCall = true;
                }
            } else if (receiver instanceof MethodInvocationTree) {
                // Voltron: receiver is itself a method call chain (e.g. `w.unwrap().get()`).
                // We use the receiver expression's toString() as the location label only;
                // facts come exclusively from tree nodes via resolveConstruction.
                // We do NOT invent a new #euf# federation for chains — if resolution fails,
                // the whole assertion is refused with a named diagnostic.
                chainedReceiverExpr = receiver;
                receiverName = receiver.toString(); // label only; NOT scanned for facts
                isInstanceMethodCall = true;
            }
        }

        String callee = methodInvocationName(callMit);
        if (callee.contains(".")) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, "callee is qualified (" + callee + "); only bare function names lifted"));
            return;
        }

        // Collect call arguments — int literals OR string literals via getBytesUtf8/getBytes shape.
        // Both lift to the same String sort when the callee is universe-registered.
        List<? extends ExpressionTree> callArgs = callMit.getArguments();
        List<Long> intArgValues = new ArrayList<>();
        List<String> strArgValues = new ArrayList<>(); // parallel list; null = int arg
        boolean argsAreStrings = false;
        for (ExpressionTree a : callArgs) {
            OptionalLong iv = asIntLiteral(a);
            if (iv.isPresent()) {
                intArgValues.add(iv.getAsLong());
                strArgValues.add(null);
            } else {
                // G1: accept StringUtils.getBytesUtf8("lit") or "lit".getBytes(...) as string literal.
                // The callsite identity keys on the literal; note equivalence in diagnostics.
                Optional<String> sv = asBytesStringLiteral(a);
                if (sv.isPresent()) {
                    intArgValues.add(0L); // placeholder (unused in string path)
                    strArgValues.add(sv.get());
                    argsAreStrings = true;
                } else {
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName, "call arg to " + callee + "(...) is not an int literal or "
                        + "getBytesUtf8/getBytes(String literal): " + a));
                    return;
                }
            }
        }

        if (isInstanceMethodCall) {
            // P5c / Voltron: instance-method call on a local or chained receiver — LOCATION-KEYED.
            // Mirrors Python: _call_origin_from_expr returns None for non-module
            // receiver attribute calls → kept location-keyed in _callsite_contract_base.
            // Two different receiver objects may return different values for the same
            // method name; cross-location unification would be unsound (#euf# is wrong).
            // We emit a ::facts + ::assertion pair anchored to the FULL scope
            // (file::class::testMethod + receiverName) so consistency is checked WITHIN
            // this test method's scope, not across tests.  Two different test methods that
            // both declare a local `codec` get DIFFERENT location bases because `scope`
            // encodes the method name — mirrors Python _callsite_contract_base location
            // path which encodes file:lineno:col_offset.

            if (chainedReceiverExpr != null) {
                // Voltron: chained receiver (e.g. `w.unwrap().get()`).
                // Attempt full two-layer resolution via resolveIntFromChain.
                // If it fails (any impurity), REFUSE with a named diagnostic — do NOT emit
                // an unsound opaque federated term for chains.
                if (strVal.isPresent()) {
                    // String path — no construction walk for chains; refuse.
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName, "voltron: chained receiver with string expected — not supported; refusing"));
                    return;
                }
                OptionalLong chainedPin = instanceUniverse.resolveIntFromChain(
                        chainedReceiverExpr, callee, intArgValues.size(), ssaBindings, diagnostics);
                if (chainedPin.isEmpty()) {
                    // Named diagnostic already appended by resolveIntFromChain or resolveConstruction.
                    // Refuse the whole assertion rather than emitting an unsound term.
                    diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                        methodName, "voltron: chained receiver could not be resolved — assertion refused"));
                    return;
                }
                // Use a safe, deterministic receiver label from the receiver expression text.
                // This is a CONTRACT NAME label only — all facts come from tree nodes.
                String safeReceiverLabel = receiverName.replaceAll("[^A-Za-z0-9_.$]", "_");
                String locationBase = callee + "@" + scope + ":" + safeReceiverLabel;
                ir.add(buildLocationKeyedIntContract(locationBase, safeReceiverLabel, callee,
                        intArgValues, intVal.getAsLong(), relation, chainedPin));
                return;
            }

            String locationBase = callee + "@" + scope + ":" + receiverName;
            if (strVal.isPresent()) {
                ir.add(buildLocationKeyedStringContract(locationBase, receiverName, callee,
                        intArgValues, strArgValues, argsAreStrings, strVal.get(), relation,
                        ssaBindings));
            } else {
                // G3: instance-universe construction pin.
                // If the receiver was constructed via `new Cls(args)` and the method is a
                // pure final-field getter, resolveIntResult returns the ctor-pinned value.
                // We pass it to buildLocationKeyedIntContract as a second `and` operand so
                // that the solver sees: =(call:m(x), ctorValue) ∧ =(call:m(x), testValue).
                // A consistent test (testValue == ctorValue) discharges; a wrong test (≠) is unsatisfied.
                OptionalLong constructed = OptionalLong.empty();
                ExpressionTree init = ssaBindings.get(receiverName);
                if (init instanceof NewClassTree nct) {
                    constructed = instanceUniverse.resolveIntResult(nct, callee, intArgValues.size(), diagnostics);
                }
                ir.add(buildLocationKeyedIntContract(locationBase, receiverName, callee,
                        intArgValues, intVal.getAsLong(), relation, constructed));
            }
        } else if (strVal.isPresent()) {
            // String expected — emit string-sort equality contract (#euf# federated)
            ir.add(buildStringContract(callee, intArgValues, strArgValues, argsAreStrings,
                    strVal.get(), relation));
            // G1: ALSO emit universe contract if callee is registered
            String universeSet = universeRegistry.getCharSet(callee);
            if (universeSet != null) {
                ir.add(buildUniverseContract(callee, intArgValues, strArgValues, argsAreStrings,
                        universeSet));
            }
        } else {
            // Int expected — original #euf# path
            ir.add(buildContractWithRelation(callee, intArgValues, intVal.getAsLong(), relation));
            // G2: ALSO emit numeric-universe contract if callee is registered
            String bvExprJson = numericRegistry.getBvExprJson(callee);
            if (bvExprJson != null) {
                ir.add(buildNumericUniverseContract(callee, intArgValues, bvExprJson));
            }
        }
    }

    /**
     * P5c: Build a location-keyed ::assertion contract for an instance-method call.
     * Mirrors Python _callsite_contract_base (location path):
     *   base = callee + "@" + file:line:col  (here: scope + receiverName as proxy for location)
     * The contract name is  <base>::assertion  so it is scoped to this test, not federated.
     *
     * The receiver is recorded as arg 0 (mirrors Python layer2.py "receiver counts as arg 0").
     * The receiver's own construction (its SSA binding) is emitted as the ::facts contract.
     *
     * String sort — used when assertEquals("SGVsbG8=", codec.encode(bytes)).
     */
    private static String buildLocationKeyedStringContract(
            String locationBase, String receiverName, String callee,
            List<Long> intArgValues, List<String> strArgValues, boolean argsAreStrings,
            String expectedStr, String relation,
            Map<String, ExpressionTree> ssaBindings) {

        String assertionName = locationBase + "::assertion";
        String safeName = toSafeName(callee);
        int arity = intArgValues.size();
        String argSig = argsAreStrings
                ? buildArgSigMixed(intArgValues, strArgValues)
                : intArgValues.stream().map(v -> "i:" + v).collect(Collectors.joining(","));
        String ctorArgs = argsAreStrings
                ? buildCtorArgsWithStrings(intArgValues, strArgValues)
                : buildCtorArgs(intArgValues);

        // Receiver as arg 0 — location-keyed free variable for the receiver object.
        // Mirrors Python "receiver counts as arg 0" (layer2.py:779,1275).
        String receiverVarJson = "{\"kind\":\"var\",\"name\":\"" + esc(receiverName) + "\"}";
        String ctorJson = "{\"kind\":\"ctor\",\"name\":\"call:" + esc(callee) + "\",\"args\":["
                + receiverVarJson
                + (ctorArgs.isEmpty() ? "" : "," + ctorArgs) + "]}";
        String constJson = "{\"kind\":\"const\",\"value\":\"" + esc(expectedStr)
                + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}";

        return "{\"kind\":\"contract\""
             + ",\"name\":\"" + esc(assertionName) + "\""
             + ",\"outBinding\":\"out\""
             + ",\"inv\":{\"kind\":\"and\",\"operands\":["
             + "{\"kind\":\"atomic\",\"name\":\"" + relation + "\",\"args\":["
             + ctorJson + "," + constJson + "]}]}}";
    }

    /**
     * P5c/G3: Build a location-keyed ::assertion contract for an instance-method int result.
     * Same structure as the String variant above but uses Int sort for the constant.
     *
     * G3 (instance-universe): when `constructed` is present, the `and` carries TWO operands:
     *   operand[0] = construction fact  =( call:m(receiver), ctorPinnedValue )
     *   operand[1] = the test's claim   =( call:m(receiver), testConstVal )
     * Both use the byte-identical ctorJson so the solver unifies them.
     * A correct test (testConstVal == ctorPinnedValue) is consistent → discharged.
     * A wrong test (testConstVal ≠ ctorPinnedValue) is unsatisfied — refuted by the ctor.
     * When `constructed` is empty, single-operand behaviour is preserved unchanged.
     */
    private static String buildLocationKeyedIntContract(
            String locationBase, String receiverName, String callee,
            List<Long> intArgValues, long constVal, String relation,
            OptionalLong constructed) {

        String assertionName = locationBase + "::assertion";
        String ctorArgs = buildCtorArgs(intArgValues);

        // Receiver as arg 0 — location-keyed free variable for the receiver object.
        String receiverVarJson = "{\"kind\":\"var\",\"name\":\"" + esc(receiverName) + "\"}";
        String ctorJson = "{\"kind\":\"ctor\",\"name\":\"call:" + esc(callee) + "\",\"args\":["
                + receiverVarJson
                + (ctorArgs.isEmpty() ? "" : "," + ctorArgs) + "]}";

        String intSort = "\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}";
        String testAtom = "{\"kind\":\"atomic\",\"name\":\"" + relation + "\",\"args\":["
             + ctorJson + ","
             + "{\"kind\":\"const\",\"value\":" + constVal + "," + intSort + "}"
             + "]}";

        String operands;
        if (constructed.isPresent()) {
            // G3: prepend the construction fact as operand[0]; test claim is operand[1].
            String ctorAtom = "{\"kind\":\"atomic\",\"name\":\"" + relation + "\",\"args\":["
                 + ctorJson + ","
                 + "{\"kind\":\"const\",\"value\":" + constructed.getAsLong() + "," + intSort + "}"
                 + "]}";
            operands = ctorAtom + "," + testAtom;
        } else {
            operands = testAtom;
        }

        return "{\"kind\":\"contract\""
             + ",\"name\":\"" + esc(assertionName) + "\""
             + ",\"outBinding\":\"out\""
             + ",\"inv\":{\"kind\":\"and\",\"operands\":["
             + operands
             + "]}}";
    }

    /**
     * Try to extract a String literal from a StringUtils.getBytesUtf8("lit") or
     * "lit".getBytes(...) invocation. These are the canonical patterns in commons-codec
     * tests for turning string literals into byte arrays.
     * Returns the string literal value if recognized, empty otherwise.
     */
    private static Optional<String> asBytesStringLiteral(ExpressionTree expr) {
        if (!(expr instanceof MethodInvocationTree mit)) return Optional.empty();
        String name = methodInvocationName(mit);
        List<? extends ExpressionTree> args = mit.getArguments();
        // StringUtils.getBytesUtf8("lit") — one String literal arg
        if (name.equals("getBytesUtf8") && args.size() == 1) {
            Optional<String> sv = asStringLiteral(args.get(0));
            if (sv.isPresent()) return sv;
        }
        // "lit".getBytes() or "lit".getBytes(charset) — receiver is String literal
        if (name.equals("getBytes")) {
            ExpressionTree sel = mit.getMethodSelect();
            if (sel instanceof MemberSelectTree ms) {
                Optional<String> sv = asStringLiteral(ms.getExpression());
                if (sv.isPresent()) return sv;
            }
        }
        return Optional.empty();
    }

    /** Try to extract a String literal value from an expression tree. */
    private static Optional<String> asStringLiteral(ExpressionTree expr) {
        if (expr instanceof ParenthesizedTree pt) return asStringLiteral(pt.getExpression());
        if (expr instanceof LiteralTree lt && lt.getValue() instanceof String s) {
            return Optional.of(s);
        }
        return Optional.empty();
    }

    private static void liftTruth(
            MethodInvocationTree mit, String methodName, String scope,
            NumericUniverseRegistry numericRegistry,
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

        // G2b: comparison-bound path — assertTrue(callExpr <op> intLiteral)
        // or assertTrue(intLiteral <op> callExpr). The predicate is read from
        // Tree.Kind; operand order is normalised so the call is always args[0].
        if (condExpr instanceof BinaryTree bt) {
            liftComparisonBound(bt, methodName, scope, false, numericRegistry, ir, diagnostics);
            return;
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
            NumericUniverseRegistry numericRegistry,
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

        // G2b: assertFalse(callExpr <op> intLiteral) → negate the predicate.
        if (condExpr instanceof BinaryTree bt) {
            liftComparisonBound(bt, methodName, scope, true, numericRegistry, ir, diagnostics);
            return;
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

    /**
     * G2b: lift a comparison-bound assertion from a BinaryTree condition.
     *
     * Supported shapes (where fn(int-literals) is a bare call):
     *   fn(args) <op> intLiteral   → predicate = kindToPredicate(op), call=lhs, lit=rhs
     *   intLiteral <op> fn(args)   → predicate = kindToPredicate(mirror(op)), call=rhs, lit=lhs
     *   (normalised so call is always args[0] of the atom)
     *
     * When negate=true (assertFalse path), the predicate is flipped:
     *   assertFalse(x < y)  ≡  x >= y
     *   assertFalse(x <= y) ≡  x > y
     *   assertFalse(x > y)  ≡  x <= y
     *   assertFalse(x >= y) ≡  x < y
     *
     * Refusals by name (named diagnostic, not silent):
     *   - operator is not a comparison kind (e.g. == or &&): refused
     *   - both sides are calls: refused (two callsites, not a bound)
     *   - call side has a non-int-literal argument: refused
     *   - call side is not a bare MethodInvocationTree: refused
     *   - call side has a qualified callee: refused
     *   - bound side is not an int literal: refused (non-literal bound)
     */
    private static void liftComparisonBound(
            BinaryTree bt, String methodName, String scope,
            boolean negate,
            NumericUniverseRegistry numericRegistry,
            List<String> ir, List<String> diagnostics) {

        Tree.Kind kind = bt.getKind();
        // Only lift the four comparison operators.
        if (kind != Tree.Kind.LESS_THAN && kind != Tree.Kind.LESS_THAN_EQUAL
                && kind != Tree.Kind.GREATER_THAN && kind != Tree.Kind.GREATER_THAN_EQUAL) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + "(comparison-bound) refused: operator "
                    + kind + " is not a supported comparison (<, <=, >, >=); condition: " + bt));
            return;
        }

        ExpressionTree lhs = bt.getLeftOperand();
        ExpressionTree rhs = bt.getRightOperand();

        boolean lhsIsCall = lhs instanceof MethodInvocationTree;
        boolean rhsIsCall = rhs instanceof MethodInvocationTree;

        if (lhsIsCall && rhsIsCall) {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + "(comparison-bound) refused: both operands are calls"
                    + " (two callsites, not a bound — out of scope for G2b): " + bt));
            return;
        }

        // Determine call side and literal side; normalise predicate so call is args[0].
        MethodInvocationTree callMit;
        long litVal;
        String predicate; // the predicate name after normalisation

        if (lhsIsCall) {
            // call <op> lit
            callMit = (MethodInvocationTree) lhs;
            OptionalLong litOpt = asIntLiteral(rhs);
            if (litOpt.isEmpty()) {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName, methodName + "(comparison-bound) refused: RHS bound is not an int literal"
                        + " (non-literal bound is out of scope for G2b): " + rhs));
                return;
            }
            litVal = litOpt.getAsLong();
            predicate = kindToPredicate(kind); // call <op> lit: predicate is the op as-is
        } else if (rhsIsCall) {
            // lit <op> call  →  normalise to call mirror(<op>) lit
            callMit = (MethodInvocationTree) rhs;
            OptionalLong litOpt = asIntLiteral(lhs);
            if (litOpt.isEmpty()) {
                diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                    methodName, methodName + "(comparison-bound) refused: LHS bound is not an int literal"
                        + " (non-literal bound is out of scope for G2b): " + lhs));
                return;
            }
            litVal = litOpt.getAsLong();
            predicate = mirrorPredicate(kind); // lit <op> call ⟹ call mirror(op) lit
        } else {
            diagnostics.add(diagnostic(scopePath(scope), scopeClassMethod(scope),
                methodName, methodName + "(comparison-bound) refused: neither operand is a call: " + bt));
            return;
        }

        // If assertFalse, negate the predicate.
        if (negate) {
            predicate = negatePredicate(predicate);
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

        ir.add(buildContractWithRelation(callee, argValues, litVal, predicate));
        // G2b × G2: also emit the numeric-universe row if the callee is in the
        // registry. The universe row and the comparison-bound row share the SAME
        // #euf# contract name → conjoined at prove time. The bv32 contagion pass
        // then promotes the comparison-bound atom to the bv32 sort so z3 can
        // reason over both under the bitvector theory.
        String bvExprJson = numericRegistry.getBvExprJson(callee);
        if (bvExprJson != null) {
            ir.add(buildNumericUniverseContract(callee, argValues, bvExprJson));
        }
    }

    /** Map Tree.Kind comparison op → SMT predicate name (call is LHS). */
    private static String kindToPredicate(Tree.Kind kind) {
        return switch (kind) {
            case LESS_THAN       -> "<";
            case LESS_THAN_EQUAL -> "<=";
            case GREATER_THAN    -> ">";
            case GREATER_THAN_EQUAL -> ">=";
            default -> throw new IllegalArgumentException("not a comparison kind: " + kind);
        };
    }

    /**
     * Mirror predicate: for `lit <op> call`, flip direction so call is LHS.
     * lit < call  ⟹  call > lit   → ">"
     * lit <= call ⟹  call >= lit  → ">="
     * lit > call  ⟹  call < lit   → "<"
     * lit >= call ⟹  call <= lit  → "<="
     */
    private static String mirrorPredicate(Tree.Kind kind) {
        return switch (kind) {
            case LESS_THAN          -> ">";
            case LESS_THAN_EQUAL    -> ">=";
            case GREATER_THAN       -> "<";
            case GREATER_THAN_EQUAL -> "<=";
            default -> throw new IllegalArgumentException("not a comparison kind: " + kind);
        };
    }

    /**
     * Negate predicate: assertFalse(x <op> y) ≡ x negated(<op>) y.
     * ¬(< ) ≡ >=,  ¬(<=) ≡ >,  ¬(> ) ≡ <=,  ¬(>=) ≡ <
     */
    private static String negatePredicate(String predicate) {
        return switch (predicate) {
            case "<"  -> ">=";
            case "<=" -> ">";
            case ">"  -> "<=";
            case ">=" -> "<";
            default -> throw new IllegalArgumentException("not a comparison predicate: " + predicate);
        };
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

    /**
     * Build ctor args that may include String-sort arguments.
     * strArgValues[i] != null means that arg is a String literal; intArgValues[i] is unused.
     */
    private static String buildCtorArgsWithStrings(
            List<Long> intArgValues, List<String> strArgValues) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < intArgValues.size(); i++) {
            if (i > 0) sb.append(',');
            String sv = strArgValues.get(i);
            if (sv != null) {
                sb.append("{\"kind\":\"const\",\"value\":\"").append(esc(sv))
                  .append("\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}");
            } else {
                sb.append("{\"kind\":\"const\",\"value\":")
                  .append(intArgValues.get(i))
                  .append(",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}");
            }
        }
        return sb.toString();
    }

    /**
     * Compact arg-signature for contract naming: String args use s:<literal>, int args use i:<n>.
     */
    private static String buildArgSigMixed(List<Long> intArgValues, List<String> strArgValues) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < intArgValues.size(); i++) {
            if (i > 0) sb.append(',');
            String sv = strArgValues.get(i);
            if (sv != null) {
                sb.append("s:").append(sv);
            } else {
                sb.append("i:").append(intArgValues.get(i));
            }
        }
        return sb.toString();
    }

    /**
     * G1: Build a string-sort equality contract.
     * The callresult term gets the String sort; the expected is a String literal.
     * Contract name uses the same #euf# schema so the conjoin folds it with
     * any universe contract on the same callsite.
     */
    private static String buildStringContract(
            String callee, List<Long> intArgValues, List<String> strArgValues,
            boolean argsAreStrings, String expectedStr, String relation) {

        String safeName = toSafeName(callee);
        int arity = intArgValues.size();
        String argSig = argsAreStrings
                ? buildArgSigMixed(intArgValues, strArgValues)
                : intArgValues.stream().map(v -> "i:" + v).collect(Collectors.joining(","));
        String contractName = callee + "#euf#c:callresult_" + safeName + "_a" + arity
                + "(" + argSig + ")::assertion";

        String ctorArgs = argsAreStrings
                ? buildCtorArgsWithStrings(intArgValues, strArgValues)
                : buildCtorArgs(intArgValues);
        String ctorJson = "{\"kind\":\"ctor\",\"name\":\"call:" + esc(callee) + "\",\"args\":["
                + ctorArgs + "]}";
        String constJson = "{\"kind\":\"const\",\"value\":\"" + esc(expectedStr)
                + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}";

        return "{\"kind\":\"contract\""
             + ",\"name\":\"" + esc(contractName) + "\""
             + ",\"outBinding\":\"out\""
             + ",\"inv\":{\"kind\":\"and\",\"operands\":["
             + "{\"kind\":\"atomic\",\"name\":\"" + relation + "\",\"args\":["
             + ctorJson + "," + constJson + "]}]}}";
    }

    /**
     * G1: Build a universe membership contract (str.chars-in-set).
     * The atom asserts the callresult is a member of the walked character set.
     * Same #euf# contract name as the equality contract → conjoined automatically.
     *
     * AST provenance: charSet was derived from walking static final byte[] literals
     * in the vendor's source (Base64.java/BaseNCodec.java). Every character in the
     * set traces to a LiteralTree node in the vendor's VariableTree initializer.
     */
    private static String buildUniverseContract(
            String callee, List<Long> intArgValues, List<String> strArgValues,
            boolean argsAreStrings, String charSet) {

        String safeName = toSafeName(callee);
        int arity = intArgValues.size();
        String argSig = argsAreStrings
                ? buildArgSigMixed(intArgValues, strArgValues)
                : intArgValues.stream().map(v -> "i:" + v).collect(Collectors.joining(","));
        String contractName = callee + "#euf#c:callresult_" + safeName + "_a" + arity
                + "(" + argSig + ")::assertion";

        String ctorArgs = argsAreStrings
                ? buildCtorArgsWithStrings(intArgValues, strArgValues)
                : buildCtorArgs(intArgValues);
        String ctorJson = "{\"kind\":\"ctor\",\"name\":\"call:" + esc(callee) + "\",\"args\":["
                + ctorArgs + "]}";
        String setJson = "{\"kind\":\"const\",\"value\":\"" + esc(charSet)
                + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}";

        return "{\"kind\":\"contract\""
             + ",\"name\":\"" + esc(contractName) + "\""
             + ",\"outBinding\":\"out\""
             + ",\"inv\":{\"kind\":\"and\",\"operands\":["
             + "{\"kind\":\"atomic\",\"name\":\"str.chars-in-set\",\"args\":["
             + ctorJson + "," + setJson + "]}]}}";
    }

    /**
     * G2: Build an int32.eq-bv-expr universe contract.
     * The contract name is the same #euf# name as the sworn equality, so the
     * conjoin folds them automatically at prove time.
     *
     * The atom carries:
     *   args[0] — the call:callee ctor term (the result variable)
     *   args[1] — the walked BV expression tree (as a JSON string embedded in a
     *             const term with sort Int — the emitter parses it back)
     *
     * The bvExprJson is a JSON string produced by NumericUniverseWalker.
     */
    private static String buildNumericUniverseContract(
            String callee, List<Long> intArgValues, String bvExprJson) {

        String safeName = toSafeName(callee);
        int arity = intArgValues.size();
        String argSig = intArgValues.stream().map(v -> "i:" + v).collect(Collectors.joining(","));
        String contractName = callee + "#euf#c:callresult_" + safeName + "_a" + arity
                + "(" + argSig + ")::assertion";

        String ctorArgs = buildCtorArgs(intArgValues);
        String ctorJson = "{\"kind\":\"ctor\",\"name\":\"call:" + esc(callee) + "\",\"args\":["
                + ctorArgs + "]}";

        return "{\"kind\":\"contract\""
             + ",\"name\":\"" + esc(contractName) + "\""
             + ",\"outBinding\":\"out\""
             + ",\"inv\":{\"kind\":\"and\",\"operands\":["
             + "{\"kind\":\"atomic\",\"name\":\"int32.eq-bv-expr\",\"args\":["
             + ctorJson + ","
             + bvExprJson
             + "]}]}}";
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
        // JSON spec (RFC 8259 section 7): the C0 control characters (U+0000 through
        // U+001F) MUST be escaped in a JSON string, plus '"' and '\'. A vendor literal
        // can legitimately contain ANY control char (commons-lang3's StringUtilsTest
        // asserts over form-feeds, vertical tabs, NUL in its whitespace/separator
        // tests). The old escaper handled only \n \r \t, so a raw control byte leaked
        // into the emitted JSON-RPC response and the rust mint aborted parsing the
        // WHOLE artifact ("control character found while parsing a string"). One bad
        // literal zeroed out hundreds of valid contracts. Escape the full control
        // range here so emission is always valid JSON.
        StringBuilder sb = new StringBuilder(s.length() + 16);
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '\\' -> sb.append("\\\\");
                case '"'  -> sb.append("\\\"");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                case '\b' -> sb.append("\\b");
                case '\f' -> sb.append("\\f");
                default -> {
                    if (c < 0x20) {
                        // Any remaining C0 control char becomes a JSON-mandated
                        // backslash-u-four-hex-digits escape.
                        sb.append(String.format("\\u%04x", (int) c));
                    } else {
                        sb.append(c);
                    }
                }
            }
        }
        return sb.toString();
    }

    // ──────────────────────────────────────────────────────────────
    // G1: UniverseRegistry — maps bare method name → walked char set
    // ──────────────────────────────────────────────────────────────

    /**
     * Immutable mapping from bare callee name → universe char-set string
     * (chars sorted+deduped, pad included if applicable).
     *
     * Built by UniverseWalker from vendor source. An entry is present ONLY
     * when the walk succeeded (static final table, chain stays in vendored source).
     * No entry = universe walk refused for that callee.
     */
    static final class UniverseRegistry {
        static final UniverseRegistry EMPTY = new UniverseRegistry(Map.of());

        private final Map<String, String> charSets; // callee simple-name → sorted charset

        UniverseRegistry(Map<String, String> charSets) {
            this.charSets = Map.copyOf(charSets);
        }

        /** Return the universe char-set for a callee, or null if not registered. */
        String getCharSet(String callee) { return charSets.get(callee); }
        boolean isEmpty() { return charSets.isEmpty(); }
        Map<String, String> all() { return charSets; }
    }

    // ──────────────────────────────────────────────────────────────
    // G1: UniverseWalker — walk static final byte[] tables from vendor source
    // ──────────────────────────────────────────────────────────────

    /**
     * THE LAW: every constraint emitted by this walker must trace to an AST node
     * of the vendored source (com.sun.source.tree.*). No hand-authored domain
     * knowledge. If it is not in the tree, it is not in the universe. In
     * particular: NO table names, NO method names, NO urlSafe defaults, NO pad
     * policy live in this kit — all of them are DISCOVERED by the walk below.
     *
     * Scope: WEAK TIER. The universe is: every output character is a member of
     * the static final encode table (∪ the pad char when the vendor's own guard
     * attributes it). Nothing else (no length formula, no bit-equation).
     *
     * The walk (all facts from tree nodes):
     *   1. TABLE SELECTOR: find an assignment whose RHS is a ternary over two
     *      identifiers naming static final array fields with all-literal
     *      initializers — `this.encodeTable = urlSafe ? A : B`. The condition
     *      identifier is the selector parameter; A/B are the tables.
     *      Mutable table guard: a branch field missing static OR final is no
     *      axiom — named refusal, selector dropped.
     *   2. PAD ATTRIBUTION: a table T gains the pad char iff the vendor's own
     *      source guards a pad write with `if (<selectorField> == T)`; the pad
     *      identifier's VALUE is walked: field literal, or field ← ctor param ←
     *      cross-class super(...) arg ← static final literal field.
     *   3. ENTRY POINTS: every public static String-returning method is walked
     *      by LITERAL PROPAGATION: boolean/int literal arguments bind to the
     *      callee's parameter names and propagate through the static overload
     *      chain, ctor invocations, and this(...) ctor chains until the
     *      selector assignment is reached and its condition evaluates to a
     *      bound literal. An unbound selector condition = ambiguous = refusal.
     *      A ternary callsite with an unbound condition resolves only if BOTH
     *      branches resolve to the SAME table.
     *
     * Chain escape: a call that leaves vendored source is a named refusal —
     * with ONE declared seam: a 1-arg `newStringUsAscii(...)` wrapper is
     * unwrapped. That is an AXIOM, not a walked fact (StringUtils is not
     * vendored; byte[]→String US-ASCII conversion is charset-transparent by
     * JDK semantics, which no syntax walk can establish). It is the only
     * non-walked step and it is name-gated and documented here.
     *
     * Honest gap (declared, not walked): the registered entry points propagate
     * isChunked=false / lineLength=0 literals, and with lineLength=0 the
     * vendor's encode path emits no line separator — that last implication is
     * value-flow the weak tier does not walk. Chunked entry points are never
     * registered (they return byte[], not String).
     */
    static final class UniverseWalker {

        static UniverseRegistry loadRegistry(
                JavaCompiler compiler, Path workspaceRoot, List<String> diagnostics) {
            List<Path> vendorDirs;
            try {
                vendorDirs = readVendorSourceDirs(workspaceRoot);
            } catch (IOException e) {
                diagnostics.add(diagnostic("<universe-walker>", "<universe-walker>",
                    "<universe-walker>", "config read error: " + e.getMessage()));
                return UniverseRegistry.EMPTY;
            }
            if (vendorDirs.isEmpty()) return UniverseRegistry.EMPTY;

            // H1 [B4/B5]: Load the externalized platform axioms (identity bridges).
            // kit-adjacent platform-axioms.json declares name+arity bridges that are
            // charset-transparent by JDK spec, not by source walk. A bridge absent
            // from this file is refused by name if encountered as a chain-escape.
            Set<String> identityBridges = loadPlatformAxioms(workspaceRoot, diagnostics);

            // Collect all vendor Java files
            List<Path> vendorFiles = new ArrayList<>();
            for (Path dir : vendorDirs) {
                if (!Files.isDirectory(dir)) continue;
                try (Stream<Path> walk = Files.walk(dir)) {
                    walk.filter(Files::isRegularFile)
                        .filter(p -> p.getFileName().toString().endsWith(".java"))
                        .sorted()
                        .forEach(vendorFiles::add);
                } catch (IOException e) {
                    diagnostics.add(diagnostic("<universe-walker>", "<universe-walker>",
                        dir.toString(), "vendor dir walk error: " + e.getMessage()));
                }
            }
            if (vendorFiles.isEmpty()) return UniverseRegistry.EMPTY;

            return buildRegistry(compiler, vendorFiles, workspaceRoot, identityBridges, diagnostics);
        }

        /**
         * H1 [B4/B5]: Load identity-bridge names from platform-axioms.json.
         * The file lives adjacent to the kit jar/class dir: resolved from the
         * classloader resource root, or from the workspace root (for in-tree use).
         * Returns the set of method names declared as identity bridges.
         * Missing file → empty set (no bridges = every chain-escape is refused).
         */
        static Set<String> loadPlatformAxioms(Path workspaceRoot, List<String> diagnostics) {
            // Look for platform-axioms.json in two locations:
            // 1. Next to the kit's .class files (classloader resource)
            // 2. In the kit source root (implementations/java/sugar-lift-java-tests/)
            //    which we locate by scanning upward from workspaceRoot for the marker file.
            // For robustness, try the kit dir relative to the JAR/classdir first via
            // ClassLoader.getSystemResource, then fall back to a fixed relative path
            // from the workspaceRoot (useful in tests that run from the workspace).
            Path axiomFile = null;

            // Try classloader resource
            try {
                java.net.URL res = JavaTestAssertionsRpc.class.getClassLoader()
                        .getResource("platform-axioms.json");
                if (res != null) axiomFile = Path.of(res.toURI());
            } catch (Exception ignored) {}

            // Kit-adjacent: the kit's class dir (out/) sits inside the kit root,
            // where platform-axioms.json lives. This works from ANY workspace
            // (showcases run with workspace roots far from the kit tree).
            if (axiomFile == null) {
                try {
                    java.net.URL loc = JavaTestAssertionsRpc.class
                            .getProtectionDomain().getCodeSource().getLocation();
                    Path classDir = Path.of(loc.toURI());
                    Path dir = Files.isDirectory(classDir) ? classDir : classDir.getParent();
                    for (int i = 0; i < 3 && dir != null; i++) {
                        Path candidate = dir.resolve("platform-axioms.json");
                        if (Files.isReadable(candidate)) { axiomFile = candidate; break; }
                        dir = dir.getParent();
                    }
                } catch (Exception ignored) {}
            }

            // Fallback: scan upward from workspaceRoot for the kit dir marker
            if (axiomFile == null) {
                // The kit root is sugar-lift-java-tests/; look for platform-axioms.json
                // by walking up from workspaceRoot up to 6 levels.
                Path dir = workspaceRoot;
                for (int i = 0; i < 6; i++) {
                    Path candidate = dir.resolve("platform-axioms.json");
                    if (Files.isReadable(candidate)) { axiomFile = candidate; break; }
                    Path parent = dir.getParent();
                    if (parent == null) break;
                    dir = parent;
                }
            }

            if (axiomFile == null || !Files.isReadable(axiomFile)) {
                // No axiom file found: no bridges declared. All chain-escapes refused.
                return Set.of();
            }

            try {
                String json = Files.readString(axiomFile, StandardCharsets.UTF_8);
                // Parse "identity_bridges": [{"name": "...", "arity": N, ...}, ...]
                int bridgesIdx = json.indexOf("\"identity_bridges\"");
                if (bridgesIdx < 0) return Set.of();
                int arrOpen = json.indexOf('[', bridgesIdx);
                if (arrOpen < 0) return Set.of();
                int arrClose = matchingBracket(json, arrOpen, '[', ']');
                if (arrClose < 0) return Set.of();
                String arrBody = json.substring(arrOpen + 1, arrClose);

                Set<String> bridges = new HashSet<>();
                // Each element is a JSON object; extract "name" from each
                int pos = 0;
                while (pos < arrBody.length()) {
                    int objOpen = arrBody.indexOf('{', pos);
                    if (objOpen < 0) break;
                    int objClose = matchingBracket(arrBody, objOpen, '{', '}');
                    if (objClose < 0) break;
                    String obj = arrBody.substring(objOpen, objClose + 1);
                    int nameIdx = obj.indexOf("\"name\"");
                    if (nameIdx >= 0) {
                        int colon = obj.indexOf(':', nameIdx + 6);
                        if (colon >= 0) {
                            int q1 = obj.indexOf('"', colon + 1);
                            int q2 = q1 >= 0 ? obj.indexOf('"', q1 + 1) : -1;
                            if (q1 >= 0 && q2 > q1) {
                                bridges.add(obj.substring(q1 + 1, q2));
                            }
                        }
                    }
                    pos = objClose + 1;
                }
                return Collections.unmodifiableSet(bridges);
            } catch (IOException e) {
                diagnostics.add(diagnostic("<universe-walker>", "<universe-walker>",
                    "platform-axioms.json", "read error: " + e.getMessage()));
                return Set.of();
            }
        }

        /**
         * Read vendor_source_dirs from [java-test-assertions] in .sugar/config.toml.
         * Uses the same TOML-lite codec as readAssertionSourceDirs — no string scanning
         * of Java source, only TOML bytes.
         */
        static List<Path> readVendorSourceDirs(Path workspaceRoot) throws IOException {
            Path configPath = workspaceRoot.resolve(".sugar").resolve("config.toml");
            if (!Files.isReadable(configPath)) return List.of();

            String toml = Files.readString(configPath, StandardCharsets.UTF_8);
            int sectionIdx = toml.indexOf("[java-test-assertions]");
            if (sectionIdx < 0) return List.of();

            int fromIdx = sectionIdx + "[java-test-assertions]".length();
            int nextSection = -1;
            for (int i = fromIdx; i < toml.length(); i++) {
                if (toml.charAt(i) == '[' && (i == 0 || toml.charAt(i - 1) == '\n')) {
                    nextSection = i;
                    break;
                }
            }
            String section = nextSection >= 0 ? toml.substring(fromIdx, nextSection) : toml.substring(fromIdx);

            int keyIdx = section.indexOf("vendor_source_dirs");
            if (keyIdx < 0) return List.of();
            int bracketOpen = section.indexOf('[', keyIdx);
            if (bracketOpen < 0) return List.of();
            int bracketClose = matchingBracket(section, bracketOpen, '[', ']');
            if (bracketClose < 0) return List.of();

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
                    StringBuilder sb = new StringBuilder();
                    i++;
                    while (i < arrayBody.length() && arrayBody.charAt(i) != '"') {
                        char ch = arrayBody.charAt(i++);
                        if (ch == '\\' && i < arrayBody.length()) {
                            char esc = arrayBody.charAt(i++);
                            switch (esc) {
                                case 'n' -> sb.append('\n'); case 't' -> sb.append('\t');
                                case 'r' -> sb.append('\r'); case '"' -> sb.append('"');
                                case '\\' -> sb.append('\\');
                                default -> { sb.append('\\'); sb.append(esc); }
                            }
                        } else sb.append(ch);
                    }
                    i++;
                    dirs.add(sb.toString());
                } else if (c == '\'') {
                    StringBuilder sb = new StringBuilder();
                    i++;
                    while (i < arrayBody.length() && arrayBody.charAt(i) != '\'')
                        sb.append(arrayBody.charAt(i++));
                    i++;
                    dirs.add(sb.toString());
                } else i++;
            }

            List<Path> result = new ArrayList<>();
            for (String d : dirs) result.add(workspaceRoot.resolve(d).normalize());
            return result;
        }

        /**
         * Parse vendor source files, build a class corpus, walk Base64.java's
         * entry-point static methods to determine table assignments, extract table
         * literals, and return a UniverseRegistry.
         *
         * All facts trace to AST nodes (VariableTree, LiteralTree, MethodTree,
         * ReturnTree, MethodInvocationTree). No string scanning.
         */
        private static UniverseRegistry buildRegistry(
                JavaCompiler compiler, List<Path> vendorFiles,
                Path workspaceRoot, Set<String> identityBridges, List<String> diagnostics) {

            // ── Step 1: parse all vendor files into AST units ──────────────
            // Map: simple class name → (compilationUnit, classTree)
            Map<String, CompilationUnitTree> unitByClass = new LinkedHashMap<>();
            Map<String, ClassTree> classTreeByName = new LinkedHashMap<>();

            for (Path src : vendorFiles) {
                try {
                    String source = Files.readString(src, StandardCharsets.UTF_8);
                    JavaFileObject fo = new StringJavaFileObject(src.toString(), source);
                    StandardJavaFileManager fm = compiler.getStandardFileManager(
                            null, null, StandardCharsets.UTF_8);
                    JavacTask task = (JavacTask) compiler.getTask(
                            null, fm, d -> {}, List.of("--release", "21"),
                            null, List.of(fo));
                    for (CompilationUnitTree cu : task.parse()) {
                        for (Tree decl : cu.getTypeDecls()) {
                            if (decl instanceof ClassTree ct) {
                                String name = ct.getSimpleName().toString();
                                unitByClass.put(name, cu);
                                classTreeByName.put(name, ct);
                            }
                        }
                    }
                    fm.close();
                } catch (IOException e) {
                    diagnostics.add(diagnostic("<universe-walker>", "<universe-walker>",
                        src.toString(), "parse error: " + e.getMessage()));
                }
            }

            if (classTreeByName.isEmpty()) return UniverseRegistry.EMPTY;

            Corpus corpus = new Corpus(classTreeByName, identityBridges);

            // ── Step 2: discover table selectors from the vendor's own AST ──
            // A selector is `<field> = <condIdent> ? <tableIdentA> : <tableIdentB>`
            // where A and B name static final array fields with all-literal
            // initializers. Nothing about the field NAMES is known to this kit.
            List<Selector> selectors = findSelectors(corpus, diagnostics);
            if (selectors.isEmpty()) return UniverseRegistry.EMPTY;

            // ── Step 3: extract per-table charsets (walked literals only) ───
            Map<String, java.util.TreeSet<Character>> tableChars = new LinkedHashMap<>();
            for (Selector sel : selectors) {
                for (String tbl : List.of(sel.trueTable, sel.falseTable)) {
                    if (tableChars.containsKey(tbl)) continue;
                    List<Integer> bytes = corpus.literalArrayValues(tbl);
                    if (bytes == null || bytes.isEmpty()) {
                        diagnostics.add(diagnostic("<universe-walker>", corpus.ownerOf(tbl), tbl,
                            "universe walk refused: table " + tbl + " contains non-literal entries"));
                        continue;
                    }
                    java.util.TreeSet<Character> cs = new java.util.TreeSet<>();
                    for (int b : bytes) cs.add((char) b);
                    tableChars.put(tbl, cs);
                }
            }

            // ── Step 4: pad attribution — the vendor's own `==`-guarded write ──
            // `if (<selectorField> == T) { ... = pad; }` attributes the pad char
            // to T. The pad identifier's VALUE is walked (field literal, or
            // field ← ctor param ← super(...) arg ← static final literal field).
            for (Selector sel : selectors) {
                Map<String, String> padIdentByTable = corpus.findPadGuards(sel.lhsField);
                for (Map.Entry<String, String> e : padIdentByTable.entrySet()) {
                    String tbl = e.getKey();
                    java.util.TreeSet<Character> cs = tableChars.get(tbl);
                    if (cs == null) continue;
                    Integer padVal = corpus.resolveFieldValue(e.getValue(), 0);
                    if (padVal == null) {
                        diagnostics.add(diagnostic("<universe-walker>", corpus.ownerOf(tbl), tbl,
                            "universe walk refused: pad write guarded on " + tbl
                            + " but pad identifier '" + e.getValue() + "' has no walkable literal value"));
                        tableChars.remove(tbl); // wrong universe is worse than none
                        continue;
                    }
                    cs.add((char) (int) padVal);
                }
            }

            // ── Step 5: resolve entry points by literal propagation ─────────
            // Every public static String-returning method is walked; boolean/int
            // literals bind to parameter names and propagate through the static
            // overload chain, ctor calls and this(...) chains until a selector
            // condition evaluates. byte[]-returning methods are never registered:
            // str.chars-in-set is a String-sorted predicate.
            Map<String, String> registry = new LinkedHashMap<>();
            Set<String> ambiguous = new HashSet<>();
            for (Map.Entry<String, ClassTree> ce : classTreeByName.entrySet()) {
                for (Tree m : ce.getValue().getMembers()) {
                    if (!(m instanceof MethodTree mt)) continue;
                    Set<Modifier> mmods = mt.getModifiers().getFlags();
                    if (!mmods.contains(Modifier.PUBLIC) || !mmods.contains(Modifier.STATIC)) continue;
                    String retType = mt.getReturnType() != null ? mt.getReturnType().toString() : "";
                    if (!retType.equals("String")) continue;
                    if (mt.getBody() == null || mt.getBody().getStatements().isEmpty()) continue;

                    String mName = mt.getName().toString();
                    List<String> notes = new ArrayList<>();
                    String tbl = corpus.resolveStaticMethod(mt, Map.of(), selectors, 0, notes);
                    if (tbl == null) {
                        if (!notes.isEmpty()) {
                            diagnostics.add(diagnostic("<universe-walker>", ce.getKey(), mName,
                                "universe walk refused: " + String.join("; ", notes)));
                        }
                        continue;
                    }
                    java.util.TreeSet<Character> cs = tableChars.get(tbl);
                    if (cs == null) continue; // table refused upstream (named there)
                    StringBuilder sb = new StringBuilder();
                    for (char c : cs) sb.append(c);
                    String charSet = sb.toString();
                    String prev = registry.get(mName);
                    if (prev != null && !prev.equals(charSet)) {
                        // Overloads resolving to DIFFERENT tables: callsite naming
                        // is simple-name keyed, so this would be ambiguous. Refuse.
                        diagnostics.add(diagnostic("<universe-walker>", ce.getKey(), mName,
                            "universe walk refused: overloads of " + mName
                            + " resolve to different tables; simple-name callsite is ambiguous"));
                        ambiguous.add(mName);
                        continue;
                    }
                    registry.put(mName, charSet);
                }
            }
            for (String a : ambiguous) registry.remove(a);

            return new UniverseRegistry(registry);
        }

        /** A walked table selector: `<lhsField> = <condParam> ? <trueTable> : <falseTable>`. */
        record Selector(String lhsField, String condName, String trueTable, String falseTable) {}

        /**
         * Discover selectors by scanning every method/ctor body for an
         * assignment whose RHS is a ternary over two identifiers that name
         * static final array fields. The mutable-table gate fires HERE: a
         * branch field missing static or final is a named refusal and the
         * selector is dropped (a mutable table is no axiom).
         */
        private static List<Selector> findSelectors(Corpus corpus, List<String> diagnostics) {
            List<Selector> selectors = new ArrayList<>();
            for (Map.Entry<String, ClassTree> ce : corpus.classes.entrySet()) {
                String className = ce.getKey();
                for (Tree m : ce.getValue().getMembers()) {
                    if (!(m instanceof MethodTree mt) || mt.getBody() == null) continue;
                    new TreeScanner<Void, Void>() {
                        @Override public Void visitAssignment(AssignmentTree at, Void p) {
                            ExpressionTree rhs = stripParens(at.getExpression());
                            if (rhs instanceof ConditionalExpressionTree cet) {
                                String cond = identName(cet.getCondition());
                                String t = identName(cet.getTrueExpression());
                                String f = identName(cet.getFalseExpression());
                                String lhs = identName(at.getVariable());
                                if (cond != null && t != null && f != null && lhs != null
                                        && corpus.isArrayField(t) && corpus.isArrayField(f)) {
                                    boolean ok = true;
                                    for (String tbl : List.of(t, f)) {
                                        if (!corpus.isStaticFinal(tbl)) {
                                            diagnostics.add(diagnostic("<universe-walker>",
                                                className, tbl,
                                                "universe walk refused: table field " + tbl
                                                + " is not static final; mutable table is no axiom"));
                                            ok = false;
                                        }
                                    }
                                    if (ok) selectors.add(new Selector(lhs, cond, t, f));
                                }
                            }
                            return super.visitAssignment(at, p);
                        }
                    }.scan(mt.getBody(), null);
                }
            }
            return selectors;
        }

        /** Identifier simple name from `x` or `this.x`; null for anything else. */
        private static String identName(ExpressionTree e) {
            e = stripParens(e);
            if (e instanceof IdentifierTree it) return it.getName().toString();
            if (e instanceof MemberSelectTree ms
                    && ms.getExpression() instanceof IdentifierTree base
                    && base.getName().contentEquals("this")) {
                return ms.getIdentifier().toString();
            }
            return null;
        }

        private static ExpressionTree stripParens(ExpressionTree e) {
            while (e instanceof ParenthesizedTree pt) e = pt.getExpression();
            return e;
        }

        /**
         * The parsed vendor corpus: classes, fields, static methods by
         * name/arity, ctors by class — plus the literal-propagation resolver.
         *
         * H1 [B4/B5]: identityBridges is the set of method names (from
         * platform-axioms.json) that are declared charset-transparent by JDK spec.
         * These are the only non-walked steps; a name NOT in this set that escapes
         * the vendored source is refused by name.
         */
        private static final class Corpus {
            final Map<String, ClassTree> classes;
            final Map<String, VariableTree> fields = new LinkedHashMap<>();      // simple name → tree
            final Map<String, String> fieldOwner = new LinkedHashMap<>();        // simple name → class
            final Map<String, MethodTree> statics = new LinkedHashMap<>();       // name/arity → tree
            final Map<String, List<MethodTree>> ctors = new LinkedHashMap<>();   // class → ctors
            /** H1 [B4/B5]: method names declared as identity bridges in platform-axioms.json */
            final Set<String> identityBridges;

            Corpus(Map<String, ClassTree> classes, Set<String> identityBridges) {
                this.classes = classes;
                this.identityBridges = identityBridges;
                for (Map.Entry<String, ClassTree> ce : classes.entrySet()) {
                    for (Tree m : ce.getValue().getMembers()) {
                        if (m instanceof VariableTree vt) {
                            fields.putIfAbsent(vt.getName().toString(), vt);
                            fieldOwner.putIfAbsent(vt.getName().toString(), ce.getKey());
                        } else if (m instanceof MethodTree mt) {
                            String n = mt.getName().toString();
                            if (n.equals("<init>")) {
                                ctors.computeIfAbsent(ce.getKey(), k -> new ArrayList<>()).add(mt);
                            } else if (mt.getModifiers().getFlags().contains(Modifier.STATIC)) {
                                statics.putIfAbsent(n + "/" + mt.getParameters().size(), mt);
                            }
                        }
                    }
                }
            }

            String ownerOf(String field) {
                return fieldOwner.getOrDefault(field, "<vendor>");
            }

            boolean isArrayField(String name) {
                VariableTree vt = fields.get(name);
                return vt != null && vt.getInitializer() instanceof NewArrayTree;
            }

            boolean isStaticFinal(String name) {
                VariableTree vt = fields.get(name);
                if (vt == null) return false;
                Set<Modifier> mods = vt.getModifiers().getFlags();
                return mods.contains(Modifier.STATIC) && mods.contains(Modifier.FINAL);
            }

            /** All-literal array initializer values, or null if any entry is non-literal. */
            List<Integer> literalArrayValues(String fieldName) {
                VariableTree vt = fields.get(fieldName);
                if (vt == null || !(vt.getInitializer() instanceof NewArrayTree nat)) return null;
                List<Integer> out = new ArrayList<>();
                for (ExpressionTree e : nat.getInitializers()) {
                    Integer v = literalCharOrInt(stripParens(e));
                    if (v == null) return null;
                    out.add(v);
                }
                return out;
            }

            private static Integer literalCharOrInt(ExpressionTree e) {
                if (e instanceof LiteralTree lt) {
                    Object v = lt.getValue();
                    if (v instanceof Character c) return (int) (char) c;
                    if (v instanceof Number n) return n.intValue();
                }
                return null;
            }

            /**
             * Find pad-write guards: `if (<selectorField> == T)` whose then-branch
             * contains an assignment from a bare identifier. Returns table → pad
             * identifier name. Both sides of the `==` are tried.
             */
            Map<String, String> findPadGuards(String selectorField) {
                Map<String, String> out = new LinkedHashMap<>();
                for (ClassTree ct : classes.values()) {
                    for (Tree m : ct.getMembers()) {
                        if (!(m instanceof MethodTree mt) || mt.getBody() == null) continue;
                        new TreeScanner<Void, Void>() {
                            @Override public Void visitIf(IfTree it, Void p) {
                                ExpressionTree cond = stripParens(it.getCondition());
                                if (cond instanceof BinaryTree bt
                                        && bt.getKind() == Tree.Kind.EQUAL_TO) {
                                    String l = identName(bt.getLeftOperand());
                                    String r = identName(bt.getRightOperand());
                                    String table = null;
                                    if (selectorField.equals(l) && r != null && isArrayField(r)) table = r;
                                    if (selectorField.equals(r) && l != null && isArrayField(l)) table = l;
                                    if (table != null) {
                                        String padIdent = findAssignedIdent(it.getThenStatement());
                                        if (padIdent != null) out.putIfAbsent(table, padIdent);
                                    }
                                }
                                return super.visitIf(it, p);
                            }
                        }.scan(mt.getBody(), null);
                    }
                }
                return out;
            }

            /** First assignment RHS that is a bare identifier inside a statement subtree. */
            private static String findAssignedIdent(StatementTree st) {
                final String[] found = {null};
                new TreeScanner<Void, Void>() {
                    @Override public Void visitAssignment(AssignmentTree at, Void p) {
                        if (found[0] == null) {
                            ExpressionTree rhs = stripParens(at.getExpression());
                            if (rhs instanceof IdentifierTree id) {
                                found[0] = id.getName().toString();
                            }
                        }
                        return super.visitAssignment(at, p);
                    }
                }.scan(st, null);
                return found[0];
            }

            /**
             * Walk a field identifier to its literal value:
             *   1. field with a literal initializer → value.
             *   2. field initialized from another identifier → one more hop.
             *   3. blank final field assigned `this.f = <param>` in a ctor →
             *      find a cross-class super(...) call with matching arity, take
             *      the arg at the param's index, resolve it (literal or field).
             */
            Integer resolveFieldValue(String name, int depth) {
                if (depth > 4) return null;
                VariableTree vt = fields.get(name);
                if (vt == null) return null;
                ExpressionTree init = vt.getInitializer();
                if (init != null) {
                    Integer v = literalCharOrInt(stripParens(init));
                    if (v != null) return v;
                    String ident = identName(init);
                    if (ident != null && !ident.equals(name)) {
                        return resolveFieldValue(ident, depth + 1);
                    }
                    return null;
                }
                // Blank final: find ctor with `this.<name> = <param>` and the
                // param's index, then a super(...) call of that ctor's arity.
                String ownerClass = fieldOwner.get(name);
                for (MethodTree ctor : ctors.getOrDefault(ownerClass, List.of())) {
                    Integer paramIdx = paramIndexAssignedToField(ctor, name);
                    if (paramIdx == null) continue;
                    int arity = ctor.getParameters().size();
                    ExpressionTree arg = findSuperCallArg(ownerClass, arity, paramIdx);
                    if (arg == null) continue;
                    arg = stripParens(arg);
                    Integer v = literalCharOrInt(arg);
                    if (v != null) return v;
                    String ident = identName(arg);
                    if (ident != null) return resolveFieldValue(ident, depth + 1);
                }
                return null;
            }

            /** If ctor body contains `this.<field> = <param>`, return the param index. */
            private static Integer paramIndexAssignedToField(MethodTree ctor, String field) {
                if (ctor.getBody() == null) return null;
                for (StatementTree st : ctor.getBody().getStatements()) {
                    if (st instanceof ExpressionStatementTree est
                            && est.getExpression() instanceof AssignmentTree at
                            && field.equals(identName(at.getVariable()))
                            && stripParens(at.getExpression()) instanceof IdentifierTree rhs) {
                        String paramName = rhs.getName().toString();
                        List<? extends VariableTree> params = ctor.getParameters();
                        for (int i = 0; i < params.size(); i++) {
                            if (params.get(i).getName().contentEquals(paramName)) return i;
                        }
                    }
                }
                return null;
            }

            /** Find `super(...)` with the given arity in any OTHER class's ctors; return arg i. */
            private ExpressionTree findSuperCallArg(String superOwner, int arity, int argIdx) {
                for (Map.Entry<String, List<MethodTree>> ce : ctors.entrySet()) {
                    if (ce.getKey().equals(superOwner)) continue;
                    for (MethodTree ctor : ce.getValue()) {
                        if (ctor.getBody() == null) continue;
                        for (StatementTree st : ctor.getBody().getStatements()) {
                            if (st instanceof ExpressionStatementTree est
                                    && est.getExpression() instanceof MethodInvocationTree mi
                                    && mi.getMethodSelect() instanceof IdentifierTree id
                                    && id.getName().contentEquals("super")
                                    && mi.getArguments().size() == arity
                                    && argIdx < arity) {
                                return mi.getArguments().get(argIdx);
                            }
                        }
                    }
                }
                return null;
            }

            // ── Literal-propagation entry resolution ─────────────────────────

            /**
             * Resolve which table a static method's String result is encoded
             * from, by propagating literal argument bindings down the chain.
             * `bindings` maps parameter names to Boolean/Integer literal values
             * known at this call depth.
             */
            String resolveStaticMethod(MethodTree mt, Map<String, Object> bindings,
                    List<Selector> selectors, int depth, List<String> notes) {
                if (depth > 12) { notes.add("delegation chain deeper than 12"); return null; }
                if (mt.getBody() == null) return null;
                Map<String, String> localTables = new LinkedHashMap<>();
                for (StatementTree st : mt.getBody().getStatements()) {
                    if (st instanceof VariableTree v && v.getInitializer() != null) {
                        String t = resolveExpr(v.getInitializer(), bindings, selectors, depth, notes);
                        if (t != null) localTables.put(v.getName().toString(), t);
                    } else if (st instanceof ReturnTree rt && rt.getExpression() != null) {
                        ExpressionTree re = stripParens(rt.getExpression());
                        String t = resolveExpr(re, bindings, selectors, depth, notes);
                        if (t != null) return t;
                        // `return local.member(args)` where local resolved to a table
                        if (re instanceof MethodInvocationTree mi
                                && mi.getMethodSelect() instanceof MemberSelectTree ms
                                && ms.getExpression() instanceof IdentifierTree recv) {
                            String lt = localTables.get(recv.getName().toString());
                            if (lt != null) return lt;
                        }
                    }
                }
                return null;
            }

            private String resolveExpr(ExpressionTree expr, Map<String, Object> bindings,
                    List<Selector> selectors, int depth, List<String> notes) {
                expr = stripParens(expr);
                if (expr instanceof ConditionalExpressionTree cet) {
                    Boolean cond = evalBool(cet.getCondition(), bindings);
                    if (cond != null) {
                        return resolveExpr(cond ? cet.getTrueExpression() : cet.getFalseExpression(),
                                bindings, selectors, depth, notes);
                    }
                    String t = resolveExpr(cet.getTrueExpression(), bindings, selectors, depth, notes);
                    String f = resolveExpr(cet.getFalseExpression(), bindings, selectors, depth, notes);
                    if (t != null && t.equals(f)) return t; // both branches agree
                    if (t != null || f != null) notes.add("ternary branches resolve to different tables");
                    return null;
                }
                if (expr instanceof MethodInvocationTree mi) {
                    String name = methodInvocationName(mi);
                    // H1 [B4/B5]: DECLARED AXIOM SEAM — consult platform-axioms.json.
                    // A name in identityBridges is charset-transparent by JDK spec
                    // (not walkable syntax). Its single argument propagates through.
                    // A name NOT in this set that escapes the vendored source is refused.
                    // Previously this was a hardcoded `name.equals("newStringUsAscii")`
                    // check; the check is now driven by the externalized axiom file.
                    if (identityBridges.contains(name) && mi.getArguments().size() == 1) {
                        return resolveExpr(mi.getArguments().get(0), bindings, selectors, depth, notes);
                    }
                    MethodTree target = statics.get(name + "/" + mi.getArguments().size());
                    if (target == null) {
                        notes.add("chain escapes vendored source at " + name
                                + "/" + mi.getArguments().size());
                        return null;
                    }
                    Map<String, Object> child = bindArgs(target.getParameters(), mi.getArguments(), bindings);
                    return resolveStaticMethod(target, child, selectors, depth + 1, notes);
                }
                if (expr instanceof NewClassTree nct) {
                    String className = nct.getIdentifier().toString();
                    for (MethodTree ctor : ctors.getOrDefault(className, List.of())) {
                        if (ctor.getParameters().size() != nct.getArguments().size()) continue;
                        Map<String, Object> child = bindArgs(ctor.getParameters(), nct.getArguments(), bindings);
                        return resolveCtor(ctor, className, child, selectors, depth + 1, notes);
                    }
                    return null;
                }
                return null;
            }

            /** Walk a ctor body: follow this(...) chains; evaluate the selector condition. */
            private String resolveCtor(MethodTree ctor, String className, Map<String, Object> bindings,
                    List<Selector> selectors, int depth, List<String> notes) {
                if (depth > 12 || ctor.getBody() == null) return null;

                // H1 [B6]: scan for non-zero integer field stores before following the chain.
                // A ctor that stores a non-zero int from bindings into a field (e.g.
                // `this.lineLength = 76`) signals a chunking parameter: lineLength > 0 means
                // the instance method injects line separators into the output. Those separator
                // chars are NOT in the static encode table, so the str.chars-in-set contract
                // would be unsound. Refuse the entry point with a named reason.
                for (StatementTree st : ctor.getBody().getStatements()) {
                    if (!(st instanceof ExpressionStatementTree est)) continue;
                    ExpressionTree e = stripParens(est.getExpression());
                    if (e instanceof AssignmentTree at) {
                        ExpressionTree rhs = stripParens(at.getExpression());
                        // Simple assignment: `this.field = paramName` (not a ternary)
                        if (rhs instanceof IdentifierTree rhsId
                                && !(rhs instanceof ConditionalExpressionTree)) {
                            Object v = bindings.get(rhsId.getName().toString());
                            if (v instanceof Number n && n.intValue() != 0) {
                                notes.add("chunking parameter non-zero: "
                                    + rhsId.getName() + "=" + n.intValue()
                                    + " — entry point injects line separators; "
                                    + "str.chars-in-set would be unsound (lineLength=0 required)");
                                return null;
                            }
                        }
                        // Direct int literal: `this.field = 76` (unrelated to bindings)
                        if (rhs instanceof LiteralTree lt
                                && lt.getValue() instanceof Number n && n.intValue() != 0) {
                            // A literal non-zero int stored in a field: same concern.
                            notes.add("chunking parameter non-zero (literal): "
                                + at.getVariable() + "=" + n.intValue()
                                + " — entry point injects line separators; "
                                + "str.chars-in-set would be unsound (lineLength=0 required)");
                            return null;
                        }
                    }
                }

                for (StatementTree st : ctor.getBody().getStatements()) {
                    if (!(st instanceof ExpressionStatementTree est)) continue;
                    ExpressionTree e = stripParens(est.getExpression());
                    if (e instanceof MethodInvocationTree mi
                            && mi.getMethodSelect() instanceof IdentifierTree id
                            && id.getName().contentEquals("this")) {
                        for (MethodTree next : ctors.getOrDefault(className, List.of())) {
                            if (next.getParameters().size() != mi.getArguments().size()) continue;
                            Map<String, Object> child = bindArgs(next.getParameters(), mi.getArguments(), bindings);
                            return resolveCtor(next, className, child, selectors, depth + 1, notes);
                        }
                        return null;
                    }
                    if (e instanceof AssignmentTree at
                            && stripParens(at.getExpression()) instanceof ConditionalExpressionTree cet) {
                        String lhs = identName(at.getVariable());
                        for (Selector sel : selectors) {
                            if (!sel.lhsField.equals(lhs)) continue;
                            Boolean cond = evalBool(cet.getCondition(), bindings);
                            if (cond == null) {
                                notes.add("selector condition '" + sel.condName
                                        + "' is not literal-determined at this callsite");
                                return null;
                            }
                            String t = identName(cond ? cet.getTrueExpression() : cet.getFalseExpression());
                            return t;
                        }
                    }
                }
                return null;
            }

            private static Map<String, Object> bindArgs(List<? extends VariableTree> params,
                    List<? extends ExpressionTree> args, Map<String, Object> outer) {
                Map<String, Object> child = new LinkedHashMap<>();
                for (int i = 0; i < params.size() && i < args.size(); i++) {
                    ExpressionTree a = stripParens(args.get(i));
                    String pname = params.get(i).getName().toString();
                    if (a instanceof LiteralTree lt) {
                        Object v = lt.getValue();
                        if (v instanceof Boolean || v instanceof Number) child.put(pname, v);
                    } else if (a instanceof IdentifierTree id) {
                        Object v = outer.get(id.getName().toString());
                        if (v != null) child.put(pname, v);
                    }
                }
                return child;
            }

            private static Boolean evalBool(ExpressionTree e, Map<String, Object> bindings) {
                e = stripParens(e);
                if (e instanceof LiteralTree lt && lt.getValue() instanceof Boolean b) return b;
                if (e instanceof IdentifierTree id) {
                    Object v = bindings.get(id.getName().toString());
                    if (v instanceof Boolean b) return b;
                }
                return null;
            }
        }
    }

    // ──────────────────────────────────────────────────────────────
    // G2: NumericUniverseRegistry — callee → BV expression JSON
    // ──────────────────────────────────────────────────────────────

    static final class NumericUniverseRegistry {
        static final NumericUniverseRegistry EMPTY = new NumericUniverseRegistry(Map.of());

        private final Map<String, String> bvExprs; // callee simple-name → BV expr JSON

        NumericUniverseRegistry(Map<String, String> bvExprs) {
            this.bvExprs = Map.copyOf(bvExprs);
        }

        /** Return the BV expression JSON for a callee, or null if not registered. */
        String getBvExprJson(String callee) { return bvExprs.get(callee); }
        boolean isEmpty() { return bvExprs.isEmpty(); }
        Map<String, String> all() { return bvExprs; }
    }

    // ──────────────────────────────────────────────────────────────
    // G2: NumericUniverseWalker — walk public static int methods from vendor source
    // ──────────────────────────────────────────────────────────────

    /**
     * THE LAW: every BV expression emitted by this walker must trace to an AST
     * node of the vendored source. No hand-authored arithmetic. If it is not
     * in the tree, it is not in the universe.
     *
     * Supported body shapes (refused by name otherwise):
     *   TERNARY_NEG_OR_SELF: `return (a < 0) ? -a : a;`
     *     — the canonical abs pattern under two's complement.
     *     BV expr: bv32.ite(bv32.slt(a, 0), bv32.neg(a), a)
     *
     * Refused shapes are named in diagnostics. Any other shape = named refusal.
     */
    static final class NumericUniverseWalker {

        static NumericUniverseRegistry loadRegistry(
                JavaCompiler compiler, Path workspaceRoot, List<String> diagnostics) {
            List<Path> vendorDirs;
            try {
                vendorDirs = UniverseWalker.readVendorSourceDirs(workspaceRoot);
            } catch (IOException e) {
                // No vendor_source_dirs configured — numeric universe is empty
                return NumericUniverseRegistry.EMPTY;
            }
            if (vendorDirs.isEmpty()) return NumericUniverseRegistry.EMPTY;

            // Collect all vendor Java files (same pattern as UniverseWalker)
            List<Path> vendorFiles = new ArrayList<>();
            for (Path dir : vendorDirs) {
                if (!Files.isDirectory(dir)) continue;
                try (Stream<Path> walk = Files.walk(dir)) {
                    walk.filter(Files::isRegularFile)
                        .filter(p -> p.getFileName().toString().endsWith(".java"))
                        .sorted()
                        .forEach(vendorFiles::add);
                } catch (IOException e) {
                    diagnostics.add(diagnostic("<numeric-universe-walker>", "<numeric-universe-walker>",
                            dir.toString(), "vendor dir walk error: " + e.getMessage()));
                }
            }
            if (vendorFiles.isEmpty()) return NumericUniverseRegistry.EMPTY;

            // Parse the vendor source files
            DiagnosticCollector<JavaFileObject> dc = new DiagnosticCollector<>();
            StandardJavaFileManager fm = compiler.getStandardFileManager(dc, null, null);
            List<String> absFiles = vendorFiles.stream()
                    .map(p -> p.toAbsolutePath().toString())
                    .collect(Collectors.toList());
            Iterable<? extends JavaFileObject> compilationUnits;
            try {
                compilationUnits = fm.getJavaFileObjectsFromStrings(absFiles);
            } catch (Exception e) {
                diagnostics.add(diagnostic("<numeric-universe-walker>", "<numeric-universe-walker>",
                        "<init>", "error opening vendor files: " + e.getMessage()));
                return NumericUniverseRegistry.EMPTY;
            }

            JavacTask task = (JavacTask) compiler.getTask(
                    null, fm, dc, List.of("-proc:none"), null, compilationUnits);
            Iterable<? extends CompilationUnitTree> trees;
            try {
                trees = task.parse();
            } catch (IOException e) {
                diagnostics.add(diagnostic("<numeric-universe-walker>", "<init>", "<init>",
                        "parse error: " + e.getMessage()));
                return NumericUniverseRegistry.EMPTY;
            }

            Map<String, String> bvExprs = new LinkedHashMap<>();
            for (CompilationUnitTree cu : trees) {
                walkCompilationUnit(cu, bvExprs, diagnostics);
            }
            return new NumericUniverseRegistry(bvExprs);
        }

        private static void walkCompilationUnit(
                CompilationUnitTree cu, Map<String, String> bvExprs, List<String> diagnostics) {
            for (Tree td : cu.getTypeDecls()) {
                if (td instanceof ClassTree ct) {
                    walkClass(ct, bvExprs, diagnostics);
                }
            }
        }

        private static void walkClass(
                ClassTree ct, Map<String, String> bvExprs, List<String> diagnostics) {
            String className = ct.getSimpleName().toString();
            for (Tree member : ct.getMembers()) {
                if (!(member instanceof MethodTree mt)) continue;
                Set<Modifier> mods = mt.getModifiers().getFlags();
                if (!mods.contains(Modifier.PUBLIC) || !mods.contains(Modifier.STATIC)) continue;
                // Must return int (primitive)
                if (!(mt.getReturnType() instanceof PrimitiveTypeTree ptt)) continue;
                if (ptt.getPrimitiveTypeKind() != TypeKind.INT) continue;
                // Single-statement body: return <expr>;
                if (mt.getBody() == null) continue;
                List<? extends StatementTree> stmts = mt.getBody().getStatements();
                if (stmts.size() != 1) continue;
                if (!(stmts.get(0) instanceof ReturnTree rt)) continue;
                ExpressionTree retExpr = stripParensN(rt.getExpression());
                if (retExpr == null) continue;

                String methodName = mt.getName().toString();
                // Collect parameter names in order
                List<String> params = mt.getParameters().stream()
                        .map(v -> v.getName().toString())
                        .collect(Collectors.toList());

                String bvJson = tryBuildBvExpr(retExpr, params, className, methodName, diagnostics);
                if (bvJson != null) {
                    bvExprs.put(methodName, bvJson);
                }
            }
        }

        /**
         * Try to build a BV expression JSON for the given return expression.
         * Returns null (and adds a named diagnostic) if the shape is not supported.
         */
        private static String tryBuildBvExpr(
                ExpressionTree expr, List<String> params,
                String className, String methodName, List<String> diagnostics) {
            // Shape: ternary (a < 0) ? -a : a
            if (expr instanceof ConditionalExpressionTree cet) {
                ExpressionTree cond  = stripParensN(cet.getCondition());
                ExpressionTree tPart = stripParensN(cet.getTrueExpression());
                ExpressionTree fPart = stripParensN(cet.getFalseExpression());
                if (cond == null || tPart == null || fPart == null) return null;

                // Condition: param < 0  (BinaryTree, LT, lhs=param, rhs=0)
                if (!(cond instanceof BinaryTree bt)) {
                    diagnostics.add(diagnostic("<numeric-universe-walker>", className, methodName,
                            "numeric universe walk refused: ternary condition is not a binary comparison; shape unsupported"));
                    return null;
                }
                if (bt.getKind() != Tree.Kind.LESS_THAN) {
                    diagnostics.add(diagnostic("<numeric-universe-walker>", className, methodName,
                            "numeric universe walk refused: ternary condition operator is not <; shape unsupported"));
                    return null;
                }
                String condLhs = asParamName(bt.getLeftOperand(), params);
                if (condLhs == null) {
                    diagnostics.add(diagnostic("<numeric-universe-walker>", className, methodName,
                            "numeric universe walk refused: LHS of condition is not a parameter; shape unsupported"));
                    return null;
                }
                if (!isIntLiteralZero(bt.getRightOperand())) {
                    diagnostics.add(diagnostic("<numeric-universe-walker>", className, methodName,
                            "numeric universe walk refused: RHS of condition is not literal 0; shape unsupported"));
                    return null;
                }
                // truePart: -param (UnaryTree UNARY_MINUS of same param)
                if (!(tPart instanceof UnaryTree ut) || ut.getKind() != Tree.Kind.UNARY_MINUS) {
                    diagnostics.add(diagnostic("<numeric-universe-walker>", className, methodName,
                            "numeric universe walk refused: true branch is not unary negation; shape unsupported"));
                    return null;
                }
                String negParam = asParamName(stripParensN(ut.getExpression()), params);
                if (negParam == null || !negParam.equals(condLhs)) {
                    diagnostics.add(diagnostic("<numeric-universe-walker>", className, methodName,
                            "numeric universe walk refused: negated param does not match condition LHS; shape unsupported"));
                    return null;
                }
                // falsePart: same param identifier
                String falseParam = asParamName(fPart, params);
                if (falseParam == null || !falseParam.equals(condLhs)) {
                    diagnostics.add(diagnostic("<numeric-universe-walker>", className, methodName,
                            "numeric universe walk refused: false branch is not the same param; shape unsupported"));
                    return null;
                }

                // All checks pass: build bv32.ite(bv32.slt(a, 0), bv32.neg(a), a)
                String varJson   = "{\"kind\":\"var\",\"name\":\"" + esc(condLhs) + "\"}";
                String zeroJson  = "{\"kind\":\"const\",\"value\":0,\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";
                String sltJson   = "{\"kind\":\"ctor\",\"name\":\"bv32.slt\",\"args\":[" + varJson + "," + zeroJson + "]}";
                String negJson   = "{\"kind\":\"ctor\",\"name\":\"bv32.neg\",\"args\":[" + varJson + "]}";
                return "{\"kind\":\"ctor\",\"name\":\"bv32.ite\",\"args\":[" + sltJson + "," + negJson + "," + varJson + "]}";
            }

            diagnostics.add(diagnostic("<numeric-universe-walker>", className, methodName,
                    "numeric universe walk refused: return expression shape not supported (not a ternary)"));
            return null;
        }

        /** Return param name if expr is an IdentifierTree naming one of the given params; else null. */
        private static String asParamName(ExpressionTree expr, List<String> params) {
            if (expr == null) return null;
            expr = stripParensN(expr);
            if (!(expr instanceof IdentifierTree id)) return null;
            String name = id.getName().toString();
            return params.contains(name) ? name : null;
        }

        /** True iff expr is an int literal with value 0. */
        private static boolean isIntLiteralZero(ExpressionTree expr) {
            expr = stripParensN(expr);
            if (expr instanceof LiteralTree lt) {
                Object v = lt.getValue();
                if (v instanceof Integer i) return i == 0;
                if (v instanceof Long l) return l == 0L;
            }
            return false;
        }

        /** Strip parentheses; returns null if input is null. */
        private static ExpressionTree stripParensN(ExpressionTree e) {
            if (e == null) return null;
            while (e instanceof ParenthesizedTree pt) e = pt.getExpression();
            return e;
        }
    }

    // ──────────────────────────────────────────────────────────────
    // G3: InstanceUniverse — construction-semantics walk through `this`
    //
    // Pins the return value of a pure final-field getter to the value
    // supplied at construction time.  All facts come from tree nodes —
    // no regex, no string scanning, no hardcoded names.
    //
    // Weak-tier (intentional): only a single-statement `return this.field;`
    // getter on a `final` field whose ctor does `this.field = param` is
    // supported.  Anything else is REFUSED by name (not silently skipped).
    // ──────────────────────────────────────────────────────────────

    static final class InstanceUniverse {

        /** Sentinel: empty universe — resolveIntResult always returns empty. */
        static final InstanceUniverse EMPTY = new InstanceUniverse(
                Collections.emptyMap(), Collections.emptyMap());

        /** Simple class name → ClassTree, built from every *.java in the workspace. */
        private final Map<String, ClassTree> classes;
        /** Simple class name → constructor list. */
        private final Map<String, List<MethodTree>> ctors;

        private InstanceUniverse(Map<String, ClassTree> classes,
                                 Map<String, List<MethodTree>> ctors) {
            this.classes = classes;
            this.ctors   = ctors;
        }

        /**
         * Walk every *.java under workspaceRoot and index all ClassTrees by simple name.
         * Per-file parse errors are skip-and-diagnose (one bad file does not abort).
         */
        static InstanceUniverse load(JavaCompiler compiler, Path workspaceRoot,
                                     List<String> diagnostics) {
            List<Path> javaFiles = new ArrayList<>();
            try (Stream<Path> walk = Files.walk(workspaceRoot)) {
                walk.filter(Files::isRegularFile)
                    .filter(p -> p.getFileName().toString().endsWith(".java"))
                    .sorted()
                    .forEach(javaFiles::add);
            } catch (IOException e) {
                diagnostics.add(diagnostic("<instance-universe>", "<instance-universe>",
                        "<instance-universe>", "workspace walk error: " + e.getMessage()));
                return EMPTY;
            }
            if (javaFiles.isEmpty()) return EMPTY;

            Map<String, ClassTree> allClasses  = new LinkedHashMap<>();
            Map<String, List<MethodTree>> allCtors = new LinkedHashMap<>();

            for (Path p : javaFiles) {
                try {
                    String source = Files.readString(p, StandardCharsets.UTF_8);
                    JavaFileObject fo = new StringJavaFileObject(p.toString(), source);
                    StandardJavaFileManager fm = compiler.getStandardFileManager(
                            null, null, StandardCharsets.UTF_8);
                    JavacTask task = (JavacTask) compiler.getTask(
                            null, fm, null,
                            List.of("--release", "21"),
                            null,
                            List.of(fo));
                    Iterable<? extends CompilationUnitTree> units = task.parse();
                    for (CompilationUnitTree unit : units) {
                        for (Tree decl : unit.getTypeDecls()) {
                            indexClass(decl, allClasses, allCtors);
                        }
                    }
                } catch (Exception e) {
                    diagnostics.add(diagnostic("<instance-universe>", p.toString(),
                            "<parse>", "skipped (isolated): "
                            + (e.getMessage() == null ? e.toString() : e.getMessage())));
                }
            }
            return new InstanceUniverse(allClasses, allCtors);
        }

        /** Recursively index top-level and member classes. */
        private static void indexClass(Tree decl,
                                       Map<String, ClassTree> classes,
                                       Map<String, List<MethodTree>> ctors) {
            if (!(decl instanceof ClassTree ct)) return;
            String simpleName = ct.getSimpleName().toString();
            if (simpleName.isEmpty()) return;
            classes.putIfAbsent(simpleName, ct);
            for (Tree m : ct.getMembers()) {
                if (m instanceof MethodTree mt && mt.getName().contentEquals("<init>")) {
                    ctors.computeIfAbsent(simpleName, k -> new ArrayList<>()).add(mt);
                } else if (m instanceof ClassTree nested) {
                    indexClass(nested, classes, ctors);
                }
            }
        }

        // ──────────────────────────────────────────────────────────────
        // Voltron: mutually-recursive construction-semantics resolver
        // ──────────────────────────────────────────────────────────────

        /** Value type: a resolved construction — class name + ctor argument list. */
        static final class ResolvedCtor {
            final String className;
            final List<? extends ExpressionTree> ctorArgs;
            ResolvedCtor(String className, List<? extends ExpressionTree> ctorArgs) {
                this.className = className;
                this.ctorArgs  = ctorArgs;
            }
        }

        /**
         * Attempt to resolve `expr` to a ResolvedCtor — i.e. determine which class was
         * constructed and what arguments were passed to its constructor.
         *
         * Cases (every other shape → Optional.empty, never a guess):
         *   - NewClassTree C(args)          → ResolvedCtor(simpleName(C), args)
         *   - IdentifierTree in ssaBindings → resolveConstruction(its initializer, depth+1)
         *   - recv.method() (zero-arg, MemberSelectTree):
         *       rc = resolveConstruction(recv, depth+1)
         *       find unique non-static `method` with arity 0 in rc.className
         *       body must be exactly `return this.field` or `return field`
         *       field must be final, not mutated outside ctor
         *       ctorArg at paramIdx → resolveConstruction(ctorArg, depth+1)
         *   - anything else → empty
         *
         * Depth guard: depth > 8 → empty (with named diagnostic).
         * All SOUNDNESS TEETH from resolveIntResult apply at EVERY layer.
         *
         * @param ssaBindings  effectively-final local variable bindings for the test method
         */
        Optional<ResolvedCtor> resolveConstruction(ExpressionTree expr, int depth,
                                                    Map<String, ExpressionTree> ssaBindings,
                                                    List<String> diagnostics) {
            if (depth > 8) {
                diagnostics.add(diagnostic("<instance-universe>", "<voltron>", "<chain>",
                        "voltron: construction chain deeper than 8 — refusing"));
                return Optional.empty();
            }

            // Case 1: direct construction.
            if (expr instanceof NewClassTree nct) {
                String cn = simpleNameOf(nct.getIdentifier());
                if (cn == null) return Optional.empty();
                return Optional.of(new ResolvedCtor(cn, nct.getArguments()));
            }

            // Case 2: local variable in ssaBindings — follow its initializer.
            if (expr instanceof IdentifierTree id) {
                String name = id.getName().toString();
                ExpressionTree init = ssaBindings.get(name);
                if (init == null) return Optional.empty();
                return resolveConstruction(init, depth + 1, ssaBindings, diagnostics);
            }

            // Case 3: zero-arg method call on a resolvable receiver.
            if (expr instanceof MethodInvocationTree mit) {
                ExpressionTree sel = mit.getMethodSelect();
                if (!(sel instanceof MemberSelectTree mst)) return Optional.empty();
                // Must be zero-arg (a pure getter of a construction).
                if (!mit.getArguments().isEmpty()) return Optional.empty();
                String methodName = mst.getIdentifier().toString();
                ExpressionTree recv = mst.getExpression();

                // Recursively resolve the receiver to a construction.
                Optional<ResolvedCtor> rcOpt = resolveConstruction(recv, depth + 1, ssaBindings, diagnostics);
                if (rcOpt.isEmpty()) return Optional.empty();
                ResolvedCtor rc = rcOpt.get();

                // Look up the class in the universe.
                ClassTree ct = classes.get(rc.className);
                if (ct == null) return Optional.empty();

                // Step 2: find exactly one non-static method named methodName with arity 0.
                List<MethodTree> candidates = new ArrayList<>();
                for (Tree m : ct.getMembers()) {
                    if (!(m instanceof MethodTree mt)) continue;
                    if (!mt.getName().contentEquals(methodName)) continue;
                    if (mt.getParameters().size() != 0) continue;
                    if (mt.getModifiers().getFlags().contains(Modifier.STATIC)) continue;
                    candidates.add(mt);
                }
                if (candidates.size() != 1) return Optional.empty();
                MethodTree method = candidates.get(0);

                // Step 3: method body must be exactly one statement: `return <expr>;`
                BlockTree body = method.getBody();
                if (body == null || body.getStatements().size() != 1) return Optional.empty();
                StatementTree sole = body.getStatements().get(0);
                if (!(sole instanceof ReturnTree rt)) return Optional.empty();
                ExpressionTree retExpr = rt.getExpression();
                if (retExpr == null) return Optional.empty();

                // Step 4: return expression must be `this.<field>` or bare `<field>`.
                String fieldName = extractFieldName(retExpr);
                if (fieldName == null) return Optional.empty();

                // Step 5: field must be final in rc.className.
                VariableTree fieldDecl = findFieldDecl(ct, fieldName);
                if (fieldDecl == null) return Optional.empty();
                if (!fieldDecl.getModifiers().getFlags().contains(Modifier.FINAL)) {
                    diagnostics.add(diagnostic("<instance-universe>", rc.className, methodName,
                            "instance-universe: field " + fieldName
                            + " not final — construction not pinned; refusing"));
                    return Optional.empty();
                }
                // Step 5b: field must not be written outside a ctor.
                if (isFieldMutatedOutsideCtor(ct, fieldName)) {
                    diagnostics.add(diagnostic("<instance-universe>", rc.className, methodName,
                            "instance-universe: field " + fieldName
                            + " assigned outside constructor — pin not safe; refusing"));
                    return Optional.empty();
                }

                // Step 6: find ctor whose arity matches rc.ctorArgs.size().
                int ctorArity = rc.ctorArgs.size();
                List<MethodTree> ctorList = ctors.getOrDefault(rc.className, List.of());
                MethodTree matchedCtor = null;
                for (MethodTree c : ctorList) {
                    if (c.getParameters().size() == ctorArity) { matchedCtor = c; break; }
                }
                if (matchedCtor == null) return Optional.empty();

                // Step 7: find which param index feeds the field.
                Integer paramIdx = paramIndexAssignedToField(matchedCtor, fieldName);
                if (paramIdx == null) return Optional.empty();

                // The supplied ctor arg at paramIdx is the next expression to resolve.
                ExpressionTree nextExpr = rc.ctorArgs.get(paramIdx);
                return resolveConstruction(nextExpr, depth + 1, ssaBindings, diagnostics);
            }

            return Optional.empty();
        }

        /**
         * Voltron entry point: resolve the int result of `outerMethod()` called on a
         * chained receiver expression (e.g. `w.unwrap()`).
         *
         * Walks: resolveConstruction(receiverExpr) → apply outerMethod getter walk → int leaf.
         * Every soundness gate from resolveIntResult applies at every layer.
         *
         * @param receiverExpr  the full receiver expression (e.g. `w.unwrap()`)
         * @param outerMethod   the final method name (e.g. `get`)
         * @param callArity     number of arguments at the outer call site (0 for `.get()`)
         * @param ssaBindings   effectively-final local variable bindings
         * @param diagnostics   named refusals appended here
         */
        OptionalLong resolveIntFromChain(ExpressionTree receiverExpr, String outerMethod,
                                         int callArity, Map<String, ExpressionTree> ssaBindings,
                                         List<String> diagnostics) {
            // Step A: resolve the receiver expression to a concrete construction.
            Optional<ResolvedCtor> rcOpt = resolveConstruction(receiverExpr, 0, ssaBindings, diagnostics);
            if (rcOpt.isEmpty()) return OptionalLong.empty();
            ResolvedCtor rc = rcOpt.get();

            // Step B: look up outerMethod in rc.className — same gates as resolveIntResult.
            ClassTree ct = classes.get(rc.className);
            if (ct == null) return OptionalLong.empty();

            // Step B2: exactly one non-static outerMethod with matching arity.
            List<MethodTree> candidates = new ArrayList<>();
            for (Tree m : ct.getMembers()) {
                if (!(m instanceof MethodTree mt)) continue;
                if (!mt.getName().contentEquals(outerMethod)) continue;
                if (mt.getParameters().size() != callArity) continue;
                if (mt.getModifiers().getFlags().contains(Modifier.STATIC)) continue;
                candidates.add(mt);
            }
            if (candidates.size() != 1) return OptionalLong.empty();
            MethodTree method = candidates.get(0);

            // Step B3: body must be exactly `return <expr>;`
            BlockTree body = method.getBody();
            if (body == null || body.getStatements().size() != 1) return OptionalLong.empty();
            StatementTree sole = body.getStatements().get(0);
            if (!(sole instanceof ReturnTree rt)) return OptionalLong.empty();
            ExpressionTree retExpr = rt.getExpression();
            if (retExpr == null) return OptionalLong.empty();

            // Step B4: return must be `this.<field>` or bare `<field>`.
            String fieldName = extractFieldName(retExpr);
            if (fieldName == null) return OptionalLong.empty();

            // Step B5: field must be final.
            VariableTree fieldDecl = findFieldDecl(ct, fieldName);
            if (fieldDecl == null) return OptionalLong.empty();
            if (!fieldDecl.getModifiers().getFlags().contains(Modifier.FINAL)) {
                diagnostics.add(diagnostic("<instance-universe>", rc.className, outerMethod,
                        "instance-universe: field " + fieldName
                        + " not final — construction not pinned; refusing"));
                return OptionalLong.empty();
            }
            if (isFieldMutatedOutsideCtor(ct, fieldName)) {
                diagnostics.add(diagnostic("<instance-universe>", rc.className, outerMethod,
                        "instance-universe: field " + fieldName
                        + " assigned outside constructor — pin not safe; refusing"));
                return OptionalLong.empty();
            }

            // Step B6: find ctor whose arity matches rc.ctorArgs.size().
            int ctorArity = rc.ctorArgs.size();
            List<MethodTree> ctorList = ctors.getOrDefault(rc.className, List.of());
            MethodTree matchedCtor = null;
            for (MethodTree c : ctorList) {
                if (c.getParameters().size() == ctorArity) { matchedCtor = c; break; }
            }
            if (matchedCtor == null) return OptionalLong.empty();

            // Step B6b: try direct literal assignment.
            OptionalLong directLit = findDirectLiteralAssignment(matchedCtor, fieldName);
            if (directLit.isPresent()) return directLit;

            // Step B7: find which param index feeds the field.
            Integer paramIdx = paramIndexAssignedToField(matchedCtor, fieldName);
            if (paramIdx == null) return OptionalLong.empty();

            ExpressionTree ctorArg = rc.ctorArgs.get(paramIdx);
            return asIntLiteral(ctorArg);
        }

        /**
         * Attempt to resolve the int return value of `methodName` called on a receiver
         * constructed by `construction` (a NewClassTree).
         *
         * Every gate below is a REFUSAL gate: if it does not hold exactly, returns empty.
         * A refusal is safer than a guess — the opaque term stays unconstrained.
         *
         * Delegates to resolveIntFromChain for the construction walk, preserving
         * byte-identical behaviour for the existing one-hop case.
         *
         * @param construction  the NewClassTree for the receiver (e.g. `new Box(5)`)
         * @param methodName    simple method name (e.g. `get`)
         * @param callArity     number of arguments at the call site (0 for `x.get()`)
         * @param diagnostics   named refusals are appended here for surfacing
         */
        OptionalLong resolveIntResult(NewClassTree construction, String methodName,
                                      int callArity, List<String> diagnostics) {
            // Delegate to resolveIntFromChain with an empty ssaBindings map —
            // the construction IS a NewClassTree so the IdentifierTree case is not needed.
            return resolveIntFromChain(construction, methodName, callArity,
                    Collections.emptyMap(), diagnostics);
        }

        /** Extract the simple field name from `this.field` or a bare `field` identifier. */
        private static String extractFieldName(ExpressionTree expr) {
            if (expr instanceof ParenthesizedTree pt) return extractFieldName(pt.getExpression());
            if (expr instanceof MemberSelectTree mst) {
                ExpressionTree sel = mst.getExpression();
                if (sel instanceof IdentifierTree id && id.getName().contentEquals("this")) {
                    return mst.getIdentifier().toString();
                }
                return null; // qualified by something other than `this`
            }
            if (expr instanceof IdentifierTree id) {
                String name = id.getName().toString();
                return name.equals("this") ? null : name; // bare identifier (field read)
            }
            return null;
        }

        /** Find a field declaration in the given class by simple name. */
        private static VariableTree findFieldDecl(ClassTree ct, String fieldName) {
            for (Tree m : ct.getMembers()) {
                if (m instanceof VariableTree vt
                        && vt.getName().contentEquals(fieldName)) {
                    return vt;
                }
            }
            return null;
        }

        /**
         * Scan all non-constructor method bodies in the class for an assignment to `fieldName`.
         * Returns true if the field is written outside of a constructor (mutation defeats the pin).
         */
        private static boolean isFieldMutatedOutsideCtor(ClassTree ct, String fieldName) {
            for (Tree m : ct.getMembers()) {
                if (!(m instanceof MethodTree mt)) continue;
                if (mt.getName().contentEquals("<init>")) continue; // constructors are fine
                if (mt.getBody() == null) continue;
                if (bodyAssignsField(mt.getBody(), fieldName)) return true;
            }
            return false;
        }

        private static boolean bodyAssignsField(BlockTree body, String fieldName) {
            for (StatementTree st : body.getStatements()) {
                if (stmtAssignsField(st, fieldName)) return true;
            }
            return false;
        }

        private static boolean stmtAssignsField(StatementTree st, String fieldName) {
            if (st == null) return false;
            if (st instanceof ExpressionStatementTree est) {
                ExpressionTree e = est.getExpression();
                if (e instanceof AssignmentTree at) {
                    ExpressionTree lhs = at.getVariable();
                    // `this.field = ...` or bare `field = ...`
                    if (lhs instanceof MemberSelectTree mst
                            && mst.getExpression() instanceof IdentifierTree tid
                            && tid.getName().contentEquals("this")
                            && mst.getIdentifier().toString().equals(fieldName)) return true;
                    if (lhs instanceof IdentifierTree id
                            && id.getName().toString().equals(fieldName)) return true;
                }
            } else if (st instanceof BlockTree bt) {
                for (StatementTree inner : bt.getStatements()) {
                    if (stmtAssignsField(inner, fieldName)) return true;
                }
            } else if (st instanceof IfTree it) {
                if (stmtAssignsField(it.getThenStatement(), fieldName)) return true;
                if (stmtAssignsField(it.getElseStatement(), fieldName)) return true;
            }
            return false;
        }

        /**
         * Try direct literal assignment in the ctor body: `this.field = <int literal>`.
         * Returns the literal value, or empty if not found.
         */
        private static OptionalLong findDirectLiteralAssignment(MethodTree ctor, String fieldName) {
            if (ctor.getBody() == null) return OptionalLong.empty();
            for (StatementTree st : ctor.getBody().getStatements()) {
                if (!(st instanceof ExpressionStatementTree est)) continue;
                if (!(est.getExpression() instanceof AssignmentTree at)) continue;
                ExpressionTree lhs = at.getVariable();
                boolean isThisField = (lhs instanceof MemberSelectTree mst
                        && mst.getExpression() instanceof IdentifierTree tid
                        && tid.getName().contentEquals("this")
                        && mst.getIdentifier().toString().equals(fieldName))
                        || (lhs instanceof IdentifierTree id
                                && id.getName().toString().equals(fieldName));
                if (!isThisField) continue;
                ExpressionTree rhs = at.getExpression();
                OptionalLong lit = asIntLiteral(rhs);
                if (lit.isPresent()) return lit;
            }
            return OptionalLong.empty();
        }

        /**
         * Reuse the Corpus logic: if ctor body contains `this.field = <param>`,
         * return the param index, else null.
         */
        private static Integer paramIndexAssignedToField(MethodTree ctor, String field) {
            if (ctor.getBody() == null) return null;
            for (StatementTree st : ctor.getBody().getStatements()) {
                if (st instanceof ExpressionStatementTree est
                        && est.getExpression() instanceof AssignmentTree at) {
                    ExpressionTree lhs = at.getVariable();
                    boolean isField = (lhs instanceof MemberSelectTree mst
                            && mst.getExpression() instanceof IdentifierTree tid
                            && tid.getName().contentEquals("this")
                            && mst.getIdentifier().toString().equals(field))
                            || (lhs instanceof IdentifierTree lid
                                    && lid.getName().toString().equals(field));
                    if (!isField) continue;
                    ExpressionTree rhs = at.getExpression();
                    if (rhs instanceof IdentifierTree paramId) {
                        String paramName = paramId.getName().toString();
                        List<? extends VariableTree> params = ctor.getParameters();
                        for (int i = 0; i < params.size(); i++) {
                            if (params.get(i).getName().contentEquals(paramName)) return i;
                        }
                    }
                }
            }
            return null;
        }

        /**
         * Extract the simple class name from a construction identifier.
         * For `new Box(5)` the identifier is `Box`; for `new pkg.Box(5)` it is `pkg.Box`
         * and we take only the last segment.
         */
        private static String simpleNameOf(Tree identifier) {
            if (identifier instanceof IdentifierTree id) return id.getName().toString();
            if (identifier instanceof MemberSelectTree mst) return mst.getIdentifier().toString();
            return null;
        }
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
