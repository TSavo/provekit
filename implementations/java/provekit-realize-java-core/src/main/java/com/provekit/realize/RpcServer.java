package com.provekit.realize;

import com.provekit.ir.Blake3;
import java.io.BufferedReader;
import java.io.File;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.TreeSet;
import java.util.jar.JarFile;
import java.util.regex.Pattern;
import java.util.stream.Collectors;
import java.util.stream.Stream;

public final class RpcServer {
    // PEP 1.7.0 sugar plugin CID for java-canonical.
    // Computed by compute_plugin_cid() over the java-canonical.json content.
    // Update this value if java-canonical.json content changes.
    static final String PLUGIN_CID =
        "blake3-512:b7ad1160f00d892d310fb33ac3372a4ebb2f89fec563cab1719e7006ab3d7593aae2162b882aedbec1b97e44957240b3c7e8ab1675456f0539c4ad3f45d22a7b";
    private static final Pattern DEPENDENCY_PROOF_NAME =
        Pattern.compile("blake3-512:[0-9a-fA-F]{128}\\.proof");

    private final BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
    private final PrintWriter out = new PrintWriter(System.out, true);
    private final JavaNullBoundaryRealizer realizer = new JavaNullBoundaryRealizer();

    public void run() {
        try {
            String line;
            while ((line = in.readLine()) != null) {
                handle(line.trim());
            }
        } catch (IOException e) {
            System.err.println("ORP RPC read error: " + e.getMessage());
        }
    }

    private void handle(String line) {
        if (line.isEmpty()) return;
        String id = JsonUtil.extractId(line);
        String method = JsonUtil.extractMethod(line);
        try {
            switch (method) {
                // PEP 1.7.0 methods
                case "provekit.plugin.describe" -> sendResponse(id, describeResult());
                case "provekit.plugin.platform_semantics" ->
                    sendResponse(id, PlatformSemanticsDeclaration.toJson());
                case "provekit.plugin.literal_encoding_answers" ->
                    sendResponse(id, LiteralEncodingAnswers.toJson());
                case "provekit.plugin.body_template_entries" -> {
                    String paramsObj = JsonUtil.extractObjectField(line, "params");
                    if (paramsObj == null) paramsObj = "{}";
                    String tag = JsonUtil.decodeJsonStringField(paramsObj, "target_library_tag");
                    if (tag == null || tag.isBlank()) {
                        tag = JsonUtil.decodeJsonStringField(paramsObj, "targetLibraryTag");
                    }
                    if (tag == null) tag = "";
                    sendResponse(id, SugarRealizer.bodyTemplateEntriesJson(tag));
                }
                case "provekit.plugin.invoke" -> {
                    // handleInvoke returns a full JSON object: {"source":..., "is_stub":...}
                    String resultObj = handleInvoke(line);
                    sendResponse(id, resultObj);
                }
                // #1375 Milestone C: target-owned assembly. Substrate sends a
                // batch of fragments + a destination hint; java decides file
                // layout (package, imports, class wrapping, helper placement)
                // and returns the files to write. Substrate stops baking
                // java's file syntax.
                case "provekit.plugin.assemble" -> {
                    String resultObj = handleAssemble(line);
                    sendResponse(id, resultObj);
                }
                case "provekit.plugin.resolve_dependency_proofs" -> {
                    try {
                        sendResponse(id, handleResolveDependencyProofs(line));
                    } catch (Exception e) {
                        String message = e.getMessage() != null ? e.getMessage() : e.getClass().getName();
                        sendError(id, -32030, "RESOLVE_DEPENDENCY_PROOFS_FAILED: " + message);
                    }
                }
                case "provekit.plugin.shutdown" -> {
                    sendResponse(id, "null");
                    System.exit(0);
                }
                // ORP v1 methods (backward compatibility)
                case "initialize" -> sendResponse(id, initResult());
                case "realize" -> {
                    RealizerPlan plan = RealizerPlan.fromJsonLine(line);
                    RealizerOutput output = realizer.realize(plan);
                    sendResponse(id, "{\"output\":" + output.toJson() + "}");
                }
                case "shutdown" -> {
                    sendResponse(id, "null");
                    System.exit(0);
                }
                default -> sendError(id, -32601, "unknown method: " + method);
            }
        } catch (Exception e) {
            sendError(id, -32000, e.getMessage() != null ? e.getMessage() : e.getClass().getName());
        }
    }

    private String handleResolveDependencyProofs(String line) throws IOException, InterruptedException {
        String paramsObj = JsonUtil.extractParamsObject(line);
        String projectRootRaw = JsonUtil.decodeJsonStringField(paramsObj, "project_root");
        if (projectRootRaw == null || projectRootRaw.isBlank()) {
            projectRootRaw = JsonUtil.decodeJsonStringField(paramsObj, "projectRoot");
        }
        Path projectRoot = projectRootRaw == null || projectRootRaw.isBlank()
            ? Paths.get("").toAbsolutePath().normalize()
            : Paths.get(projectRootRaw).toAbsolutePath().normalize();
        List<Path> classpath = mavenDependencyClasspath(projectRoot);
        List<Path> proofs = collectDependencyProofPaths(classpath);

        StringBuilder out = new StringBuilder("{\"proof_paths\":[");
        for (int i = 0; i < proofs.size(); i++) {
            if (i > 0) out.append(',');
            out.append(JsonUtil.quoted(proofs.get(i).toString()));
        }
        out.append("]}");
        return out.toString();
    }

    private static List<Path> mavenDependencyClasspath(Path projectRoot)
            throws IOException, InterruptedException {
        Path pom = projectRoot.resolve("pom.xml");
        if (!Files.isRegularFile(pom)) return List.of();

        Path output = Files.createTempFile("provekit-java-dependency-classpath-", ".txt");
        try {
            ProcessBuilder builder = new ProcessBuilder(
                "mvn",
                "-q",
                "-B",
                "-ntp",
                "-f",
                pom.toString(),
                "dependency:build-classpath",
                "-Dmdep.outputFile=" + output
            );
            builder.directory(projectRoot.toFile());
            builder.redirectErrorStream(true);
            Process process = builder.start();
            String log;
            try (BufferedReader reader = new BufferedReader(new InputStreamReader(
                    process.getInputStream(), StandardCharsets.UTF_8))) {
                log = reader.lines().collect(Collectors.joining("\n"));
            }
            int status = process.waitFor();
            if (status != 0) {
                String suffix = log.isBlank() ? "" : ": " + log.trim();
                throw new IOException("mvn dependency:build-classpath failed with exit " + status + suffix);
            }
            if (!Files.isRegularFile(output)) return List.of();
            return parseClasspath(Files.readString(output, StandardCharsets.UTF_8));
        } finally {
            try {
                Files.deleteIfExists(output);
            } catch (IOException ignored) {
                // Temp-file cleanup is best-effort.
            }
        }
    }

    private static List<Path> parseClasspath(String classpath) {
        if (classpath == null || classpath.isBlank()) return List.of();
        LinkedHashSet<Path> roots = new LinkedHashSet<>();
        for (String part : classpath.trim().split(Pattern.quote(File.pathSeparator))) {
            if (part == null || part.isBlank()) continue;
            roots.add(Paths.get(part).toAbsolutePath().normalize());
        }
        return List.copyOf(roots);
    }

    private static List<Path> collectDependencyProofPaths(List<Path> classpath) throws IOException {
        TreeSet<Path> proofs = new TreeSet<>(Comparator.comparing(Path::toString));
        Path extractedRoot = null;
        int jarIndex = 0;
        for (Path root : classpath) {
            if (Files.isDirectory(root)) {
                collectDirectoryProofPaths(root, proofs);
            } else if (Files.isRegularFile(root) && root.getFileName().toString().endsWith(".jar")) {
                if (extractedRoot == null) {
                    extractedRoot = Files.createTempDirectory("provekit-java-dependency-proofs-");
                }
                collectJarProofPaths(root, extractedRoot.resolve(Integer.toString(jarIndex++)), proofs);
            }
        }
        return List.copyOf(proofs);
    }

    private static void collectDirectoryProofPaths(Path root, TreeSet<Path> proofs) throws IOException {
        try (Stream<Path> paths = Files.walk(root)) {
            paths
                .filter(Files::isRegularFile)
                .filter(path -> isDependencyProofName(path.getFileName().toString()))
                .map(path -> path.toAbsolutePath().normalize())
                .forEach(proofs::add);
        }
    }

    private static void collectJarProofPaths(Path jar, Path extractedRoot, TreeSet<Path> proofs)
            throws IOException {
        Files.createDirectories(extractedRoot);
        try (JarFile jarFile = new JarFile(jar.toFile())) {
            var entries = jarFile.entries();
            while (entries.hasMoreElements()) {
                var entry = entries.nextElement();
                if (entry.isDirectory()) continue;
                String fileName = jarEntryFileName(entry.getName());
                if (!isDependencyProofName(fileName)) continue;
                Path out = extractedRoot.resolve(fileName).toAbsolutePath().normalize();
                try (var in = jarFile.getInputStream(entry)) {
                    Files.copy(in, out, java.nio.file.StandardCopyOption.REPLACE_EXISTING);
                }
                proofs.add(out);
            }
        }
    }

    private static String jarEntryFileName(String entryName) {
        int slash = entryName.lastIndexOf('/');
        return slash >= 0 ? entryName.substring(slash + 1) : entryName;
    }

    private static boolean isDependencyProofName(String fileName) {
        return fileName != null && DEPENDENCY_PROOF_NAME.matcher(fileName).matches();
    }

    /**
     * Handle provekit.plugin.invoke.
     *
     * Params (from the JSON-RPC request "params" object):
     *   function      - snake_case function name
     *   params        - JSON array of parameter name strings
     *   param_types   - JSON array of source-language type strings
     *   return_type   - source-language return type string
     *   concept_name  - concept binding name for annotation + stub body
     *
     * Returns: JSON object with `source` (Java string) and `is_stub` (boolean).
     * `is_stub=true` means the body fell through to the language stub
     * (no body-template matched); `is_stub=false` means a body-template
     * entry rendered a real body. cmd_bind uses this to emit accurate
     * per-concept `bind-stub-body-emitted` gap entries per body-template-memento.md §5.
     */
    /**
     * #1375 Milestone C: target-owned compilation-unit assembly.
     *
     * Substrate sends the kit the fragments it collected for one source
     * file + a destination hint (file_basename, optional package_hint).
     * The kit decides:
     *   - file names (may split into multiple files)
     *   - package declaration
     *   - import block (dedupe across fragments)
     *   - class wrapping (one or many)
     *   - helper placement (static fields, init blocks)
     *
     * Returns a list of {path, content} pairs that the substrate writes
     * verbatim to the out-dir. The substrate stops baking java's file
     * syntax — that decision lives here now.
     *
     * Request shape:
     *   {"target_lang":"java","file_basename":"lib","package_hint":"...",
     *    "fragments":[{
     *       "concept_name":"...",
     *       "source":"...",
     *       "imports":[...],
     *       "helpers":[...],
     *       "dependencies":[...],
     *       "diagnostics":[...],
     *       "compile_unit_requirements":{...}
     *    }, ...]}
     *
     * Response shape:
     *   {"files":[{"path":"Lib.java","content":"..."}]}
     */
    private String handleAssemble(String line) {
        String paramsObj = JsonUtil.extractParamsObject(line);
        String fileBasename = JsonUtil.decodeJsonStringField(paramsObj, "file_basename");
        if (fileBasename == null || fileBasename.isBlank()) fileBasename = "lib";
        String packageHint = JsonUtil.decodeJsonStringField(paramsObj, "package_hint");
        String fragmentsJson = JsonUtil.extractArrayField(paramsObj, "fragments");

        // Parse fragments + collect imports/sources/helpers.
        java.util.TreeSet<String> mergedImports = new java.util.TreeSet<>();
        java.util.LinkedHashSet<String> mergedHelpers = new java.util.LinkedHashSet<>();
        java.util.List<String> bodies = new java.util.ArrayList<>();
        try {
            com.provekit.ir.Jcs.Json doc = com.provekit.ir.Jcs.parse(fragmentsJson);
            if (doc instanceof com.provekit.ir.Jcs.Arr arr) {
                for (com.provekit.ir.Jcs.Json item : arr.values()) {
                    if (!(item instanceof com.provekit.ir.Jcs.Obj o)) continue;
                    String src = o.stringFieldOrNull("source");
                    if (src != null && !src.isBlank()) bodies.add(src);
                    com.provekit.ir.Jcs.Json importsArr = o.get("imports");
                    if (importsArr instanceof com.provekit.ir.Jcs.Arr ia) {
                        for (com.provekit.ir.Jcs.Json v : ia.values()) {
                            if (v instanceof com.provekit.ir.Jcs.Str s) {
                                String fqn = s.value();
                                if (!fqn.startsWith("java.lang.")) mergedImports.add(fqn);
                            }
                        }
                    }
                    // #1390: collect helpers from each fragment.
                    com.provekit.ir.Jcs.Json helpersArr = o.get("helpers");
                    if (helpersArr instanceof com.provekit.ir.Jcs.Arr ha) {
                        for (com.provekit.ir.Jcs.Json v : ha.values()) {
                            if (v instanceof com.provekit.ir.Jcs.Str s) {
                                mergedHelpers.add(s.value());
                            }
                        }
                    }
                }
            }
        } catch (RuntimeException ignored) {
            // Substrate-honest: malformed fragments → empty compilation unit.
        }

        // Class name: PascalCase from file basename.
        String className = toPascalCase(fileBasename);
        StringBuilder out = new StringBuilder();
        if (packageHint != null && !packageHint.isBlank()) {
            out.append("package ").append(packageHint).append(";\n\n");
        }
        for (String imp : mergedImports) {
            out.append("import ").append(imp).append(";\n");
        }
        if (!mergedImports.isEmpty()) out.append('\n');
        out.append("public final class ").append(className).append(" {\n");
        // #1390: emit helpers (static field declarations) before methods.
        // Deduplicated by exact source text across fragments.
        for (String helper : mergedHelpers) {
            out.append("    ").append(helper).append('\n');
        }
        if (!mergedHelpers.isEmpty()) out.append('\n');
        for (int i = 0; i < bodies.size(); i++) {
            String body = bodies.get(i);
            // Strip outer wrapper class if the fragment came pre-wrapped.
            // Realizers historically emit `final class FooTransported { method }`;
            // the assembler peels that to get just the methods. Detected by
            // a `class` keyword followed by `{` on the first non-comment line.
            String unwrapped = stripWrappingClass(body);
            for (String line2 : unwrapped.split("\n", -1)) {
                if (line2.isEmpty()) {
                    out.append('\n');
                } else {
                    out.append("    ").append(line2).append('\n');
                }
            }
            if (i + 1 < bodies.size()) out.append('\n');
        }
        out.append("}\n");

        // #1388: emit the kit's runtime classpath so the substrate's
        // --compile-check can invoke javac with the JARs the materialized
        // code references (jackson, bouncycastle, etc.).
        //
        // Sources:
        //   1. The plugin's java.class.path (the realize-java jar with its
        //      shaded dependencies).
        //   2. Known maven-cached dependency JARs (jackson, bouncycastle)
        //      that the materialized code references but the realize plugin
        //      doesn't load itself. Stop-gap until proper Maven-coord
        //      resolution lands on the substrate side.
        java.util.LinkedHashSet<String> classpath = new java.util.LinkedHashSet<>();
        String javaCp = System.getProperty("java.class.path", "");
        if (!javaCp.isEmpty()) {
            String sep = System.getProperty("path.separator", ":");
            for (String e : javaCp.split(java.util.regex.Pattern.quote(sep))) {
                if (!e.isEmpty()) classpath.add(e);
            }
        }
        // Scan the user's local Maven repo for the JARs the materialized
        // code is likely to need. Picks the highest available version of
        // each artifact under ~/.m2/repository/. Substrate-honest version
        // pinning belongs in a per-kit dependencies list (next iteration).
        String userHome = System.getProperty("user.home", "");
        if (!userHome.isEmpty()) {
            java.nio.file.Path m2 = java.nio.file.Paths.get(userHome, ".m2", "repository");
            scanMavenJars(m2, "com/fasterxml/jackson/core/jackson-databind", classpath);
            scanMavenJars(m2, "com/fasterxml/jackson/core/jackson-core", classpath);
            scanMavenJars(m2, "com/fasterxml/jackson/core/jackson-annotations", classpath);
            scanMavenJars(m2, "org/bouncycastle/bcprov-jdk18on", classpath);
            scanMavenJars(m2, "org/bouncycastle/bcprov-jdk15on", classpath);
            scanMavenJars(m2, "com/google/code/gson/gson", classpath);
            scanMavenJars(m2, "org/xerial/sqlite-jdbc", classpath);
        }
        StringBuilder cpJson = new StringBuilder("[");
        boolean first = true;
        for (String e : classpath) {
            if (!first) cpJson.append(',');
            cpJson.append(JsonUtil.quoted(e));
            first = false;
        }
        cpJson.append(']');

        // Single file response for now.
        String filePath = className + ".java";
        return "{\"files\":[{"
            + "\"path\":" + JsonUtil.quoted(filePath)
            + ",\"content\":" + JsonUtil.quoted(out.toString())
            + "}],\"compile_classpath\":" + cpJson
            + "}";
    }

    /**
     * #1388: scan ~/.m2/repository/<group>/<artifact>/ for the most-recent
     * version's JAR and add it to the classpath set. Picks the
     * lexicographically-latest version directory. Silent on absence — the
     * substrate's compile-check will surface missing JARs as
     * package-not-found errors.
     */
    private static void scanMavenJars(
            java.nio.file.Path m2Root,
            String artifactDir,
            java.util.Collection<String> classpath) {
        if (m2Root == null || !java.nio.file.Files.isDirectory(m2Root)) return;
        java.nio.file.Path artifactPath = m2Root.resolve(artifactDir);
        if (!java.nio.file.Files.isDirectory(artifactPath)) return;
        try (java.util.stream.Stream<java.nio.file.Path> versions =
                java.nio.file.Files.list(artifactPath)) {
            java.util.List<java.nio.file.Path> dirs = versions
                .filter(java.nio.file.Files::isDirectory)
                .sorted(java.util.Comparator.reverseOrder())
                .toList();
            for (java.nio.file.Path versionDir : dirs) {
                String artifactBase = artifactDir.substring(artifactDir.lastIndexOf('/') + 1);
                String jarName = artifactBase + "-" + versionDir.getFileName().toString() + ".jar";
                java.nio.file.Path jar = versionDir.resolve(jarName);
                if (java.nio.file.Files.isRegularFile(jar)) {
                    classpath.add(jar.toAbsolutePath().toString());
                    return;  // first (latest) match wins
                }
            }
        } catch (Exception ignored) {
            // Best-effort: missing dep manifests as javac error downstream.
        }
    }

    /** PascalCase from snake-case or kebab-case file basename. */
    private static String toPascalCase(String basename) {
        StringBuilder sb = new StringBuilder();
        boolean upNext = true;
        for (int i = 0; i < basename.length(); i++) {
            char c = basename.charAt(i);
            if (c == '_' || c == '-' || c == '.') {
                upNext = true;
            } else if (upNext) {
                sb.append(Character.toUpperCase(c));
                upNext = false;
            } else {
                sb.append(c);
            }
        }
        return sb.length() == 0 ? "Lib" : sb.toString();
    }

    /**
     * Strip an outer `final class Foo { ... }` wrapper if present, returning
     * just the inner body. Realizers historically wrap each method in a
     * per-concept final class; the assembler collects them into one outer
     * class, so the inner wrappers must be peeled.
     *
     * Returns the original body unchanged if no wrapper is detected.
     */
    private static String stripWrappingClass(String body) {
        String trimmed = body.trim();
        // Look for "final class <Name> {" near the start, optionally
        // preceded by comments.
        java.util.regex.Pattern p = java.util.regex.Pattern.compile(
            "(?s)^\\s*(?://[^\\n]*\\n\\s*)*(?:final\\s+|public\\s+)?class\\s+\\w+\\s*\\{(.*)\\}\\s*$"
        );
        java.util.regex.Matcher m = p.matcher(trimmed);
        if (m.matches()) {
            return m.group(1).trim();
        }
        return body;
    }

    private String handleInvoke(String line) {
        // Extract the inner params object to avoid ambiguity with the RPC "params" key.
        String paramsObj = JsonUtil.extractParamsObject(line);
        String function = JsonUtil.decodeJsonStringField(paramsObj, "function");
        String sourceFunctionName = JsonUtil.decodeJsonStringField(paramsObj, "source_function_name");
        if (sourceFunctionName.isBlank()) {
            sourceFunctionName = JsonUtil.decodeJsonStringField(paramsObj, "sourceFunctionName");
        }
        String emittedFunction = sourceFunctionName.isBlank() ? function : sourceFunctionName;
        String returnType = JsonUtil.decodeJsonStringField(paramsObj, "return_type");
        String conceptName = JsonUtil.decodeJsonStringField(paramsObj, "concept_name");
        String mode = JsonUtil.decodeJsonStringField(paramsObj, "mode");
        List<String> modes = JsonUtil.decodeJsonStringArray(paramsObj, "modes");
        ContractPayload contract = ContractPayload.fromJson(JsonUtil.extractObjectField(paramsObj, "contract"));
        TransportedOperation transportedOp = TransportedOperation.fromJson(JsonUtil.extractObjectField(paramsObj, "transported_op"));
        if (transportedOp == null) {
            String namedTermTree = JsonUtil.extractObjectField(paramsObj, "named_term_tree");
            if ("{}".equals(namedTermTree)) {
                namedTermTree = JsonUtil.extractObjectField(paramsObj, "namedTermTree");
            }
            transportedOp = TransportedOperation.fromNamedTermTree(namedTermTree);
        }
        String termShape = JsonUtil.extractObjectField(paramsObj, "term_shape");
        if ("{}".equals(termShape)) {
            termShape = JsonUtil.extractObjectField(paramsObj, "termShape");
        }
        String operandBindings = JsonUtil.extractArrayField(paramsObj, "operand_bindings");
        if ("[]".equals(operandBindings)) {
            operandBindings = JsonUtil.extractArrayField(paramsObj, "operandBindings");
        }
        List<String> sugarPlugins = JsonUtil.decodeJsonObjectArray(paramsObj, "sugar_plugins");
        List<String> params = JsonUtil.decodeJsonStringArray(paramsObj, "params");
        List<String> paramTypes = JsonUtil.decodeJsonStringArray(paramsObj, "param_types");
        // Substrate-honest cross-language signature pins: concept-hub sort
        // CIDs flow through the carrier from the SOURCE kit's lift. The
        // target (java) realize binary uses them to resolve java syntax
        // via its own catalog — no per-(source, target) translation table.
        List<String> paramSortCids = JsonUtil.decodeJsonStringArray(paramsObj, "param_sort_cids");
        String returnSortCid = JsonUtil.decodeJsonStringField(paramsObj, "return_sort_cid");
        if (returnSortCid == null) returnSortCid = "";
        // Cross-language signaling discriminator: explicit field presence on
        // the RPC params object. EITHER param_sort_cids OR return_sort_cid
        // declared in the payload → caller is cross-lang and any empty CID
        // means "substrate gap; refuse loudly". Field-absent means same-lang
        // / legacy → empties are absence-of-signal, not declared gap.
        boolean isCrossLang = JsonUtil.hasField(paramsObj, "param_sort_cids")
                || JsonUtil.hasField(paramsObj, "return_sort_cid");
        // Dispatcher-resolved library_tag for body-template disambiguation.
        // Absent → "" → matcher only considers library-agnostic catch-all entries.
        String targetLibraryTag = JsonUtil.decodeJsonStringField(paramsObj, "target_library_tag");
        if (targetLibraryTag == null) targetLibraryTag = "";
        // #1369: parametric content-addressing expansions for composite sort CIDs.
        // Each expansion declares (composite_cid → constructor_cid + arg_cids)
        // so SugarRealizer can decompose composite CIDs for parameterized
        // morphism dispatch.
        java.util.List<SugarRealizer.ParametricExpansion> parametricExpansions = parseParametricExpansions(
                JsonUtil.extractArrayField(paramsObj, "parametric_sort_expansions"));
        // Function-return-type catalog from the cross-term pre-pass.
        // Substrate-honest: lower passes ALL terms' return types so
        // call expressions inside a term can pick up real types from
        // sibling terms instead of falling back to var inference.
        java.util.Map<String, String> functionReturnTypes = parseFunctionReturnTypes(
                JsonUtil.extractObjectField(paramsObj, "function_return_types"));
        SugarRealizer.currentCallReturnTypes.set(functionReturnTypes);
        // Source-language signature metadata (visibility, generic params,
        // original param types). The lower passes these so the @sugar header
        // comment can carry them — the java lift then recovers them for
        // round-trip back to the source language WITHOUT external metadata
        // injection at integration time.
        // RealizeRequest serializes via serde's default snake_case for
        // unrenamed fields. Try both snake_case and camelCase for safety
        // across spec construction paths (some use rename = camelCase).
        String sourceVisibility = JsonUtil.decodeJsonStringField(paramsObj, "visibility");
        if (sourceVisibility == null) sourceVisibility = "";
        String sourceGenericParams = JsonUtil.decodeJsonStringField(paramsObj, "generic_params");
        if (sourceGenericParams == null || sourceGenericParams.isEmpty()) {
            sourceGenericParams = JsonUtil.decodeJsonStringField(paramsObj, "genericParams");
        }
        if (sourceGenericParams == null) sourceGenericParams = "";
        java.util.List<String> sourceOriginalParamTypes =
                JsonUtil.decodeJsonStringArray(paramsObj, "original_param_types");
        if (sourceOriginalParamTypes == null || sourceOriginalParamTypes.isEmpty()) {
            sourceOriginalParamTypes =
                JsonUtil.decodeJsonStringArray(paramsObj, "originalParamTypes");
        }
        SugarRealizer.currentSourceVisibility.set(sourceVisibility);
        SugarRealizer.currentSourceGenericParams.set(sourceGenericParams);
        SugarRealizer.currentSourceOriginalParamTypes.set(sourceOriginalParamTypes);
        // Source doc comment lines (after the @sugar attribute). The
        // lower passes them as `docLines`; @substrate-signature embeds
        // them so the java lift can restore for the cycle round-trip.
        java.util.List<String> sourceDocLines =
                JsonUtil.decodeJsonStringArray(paramsObj, "doc_lines");
        if (sourceDocLines == null || sourceDocLines.isEmpty()) {
            sourceDocLines = JsonUtil.decodeJsonStringArray(paramsObj, "docLines");
        }
        if (sourceDocLines == null) sourceDocLines = java.util.List.of();
        SugarRealizer.currentSourceDocLines.set(sourceDocLines);
        // Carry the source-language term_shape verbatim so the @sugar
        // header can embed it for round-trip. This is the authoritative
        // structural form — the java body_shape (re-derived from AST)
        // would only be a target-language idiom. The substrate cycle
        // needs the SOURCE's term_shape preserved as data.
        SugarRealizer.currentSourceTermShape.set(termShape == null ? "" : termShape);
        SugarRealizer.Realization r;
        try {
            r = SugarRealizer.emitStub(emittedFunction, params, paramTypes, paramSortCids, returnType, returnSortCid,
                    conceptName, mode, modes, contract, sugarPlugins, transportedOp, termShape, operandBindings,
                    isCrossLang, targetLibraryTag, parametricExpansions);
        } finally {
            SugarRealizer.currentCallReturnTypes.remove();
            SugarRealizer.currentSourceVisibility.remove();
            SugarRealizer.currentSourceGenericParams.remove();
            SugarRealizer.currentSourceOriginalParamTypes.remove();
            SugarRealizer.currentSourceTermShape.remove();
        }
        String wrapperRecord = r.observationWrapperEmissionRecord() == null
                ? ""
                : ",\"observation_wrapper_emission_record\":" + r.observationWrapperEmissionRecord();
        // #1374: extract FQN imports from the emitted source. Substrate-side
        // assembly (Milestone C) deduplicates these and emits the idiomatic
        // import block for the target language. The body itself can keep
        // FQN-inline references (compiles either way); the imports field
        // lets downstream tooling know what the fragment USES.
        String importsJson = importsFromSource(r.source());
        // #1390: emit helpers as a structured field. The assembler hoists
        // them into the compilation unit before methods.
        StringBuilder helpersJson = new StringBuilder("[");
        boolean firstHelper = true;
        for (String h : r.helpers()) {
            if (!firstHelper) helpersJson.append(',');
            helpersJson.append(JsonUtil.quoted(h));
            firstHelper = false;
        }
        helpersJson.append(']');
        return "{\"kind\":\"realization-fragment\""
                + ",\"source\":" + JsonUtil.quoted(r.source())
                + ",\"emitted_artifact_cid\":"
                + JsonUtil.quoted(Blake3.blake3_512(r.source().getBytes(StandardCharsets.UTF_8)))
                + ",\"is_stub\":" + (r.isStub() ? "true" : "false")
                + ",\"observed_loss_record\":" + r.observedLossRecord()
                + ",\"used_sugars\":" + r.usedSugarsJson()
                + ",\"imports\":" + importsJson
                + ",\"helpers\":" + helpersJson
                + wrapperRecord
                + "}";
    }

    /**
     * #1374: extract java FQN imports from the emitted source.
     *
     * Pattern: lowercase package segments separated by dots, then a
     * PascalCase class name. Matches `com.fasterxml.jackson.databind.JsonNode`,
     * `java.util.List`, `java.io.ByteArrayOutputStream`. Skips inner class
     * suffixes (the matcher captures up to the FIRST PascalCase identifier;
     * `JsonNode.NumberType` matches just `JsonNode`).
     *
     * Returns a JSON array of unique FQN strings sorted lexicographically.
     */
    private static String importsFromSource(String source) {
        if (source == null || source.isEmpty()) return "[]";
        java.util.regex.Pattern p = java.util.regex.Pattern.compile(
            "\\b([a-z][a-z0-9_]*(?:\\.[a-z][a-z0-9_]*)+\\.[A-Z][A-Za-z0-9_]*)"
        );
        java.util.regex.Matcher m = p.matcher(source);
        java.util.TreeSet<String> imports = new java.util.TreeSet<>();
        while (m.find()) {
            String fqn = m.group(1);
            // Skip java.lang.* — implicit in every compilation unit.
            if (fqn.startsWith("java.lang.")) continue;
            imports.add(fqn);
        }
        StringBuilder sb = new StringBuilder("[");
        boolean first = true;
        for (String fqn : imports) {
            if (!first) sb.append(',');
            sb.append(JsonUtil.quoted(fqn));
            first = false;
        }
        sb.append(']');
        return sb.toString();
    }

    /**
     * PEP 1.7.0 provekit.plugin.describe result.
     *
     * Returns the java-canonical sugar plugin memento (without envelope/metadata;
     * those are loader-level fields). The result IS the plugin memento body per §4.2.1.
     */
    private String describeResult() {
        // The content payload mirrors java-canonical.json header.content.
        // The CID is pre-computed and matches the fixture file.
        return "{"
            + "\"envelope\":{"
            + "\"declaredAt\":\"2026-05-12T00:00:00.000Z\","
            + "\"signature\":\"ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\","
            + "\"signer\":\"ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\""
            + "},"
            + "\"header\":{"
            + "\"cid\":" + JsonUtil.quoted(PLUGIN_CID) + ","
            + "\"content\":" + contentJson() + ","
            + "\"critical\":false,"
            + "\"kind\":\"sugar\","
            + "\"protocol_versions\":[\"pep/1.7.0\"],"
            + "\"provenance_cid\":\"blake3-512:0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\","
            + "\"schemaVersion\":\"1\","
            + "\"version\":\"1.0.0\""
            + "},"
            + "\"metadata\":{"
            + "\"note\":\"Canonical Java annotation sugar dict for ProvekIt contract clause rendering.\","
            + "\"source_url\":\"menagerie/java-language-signature/specs/sugar/java-canonical.json\""
            + "}"
            + "}";
    }

    /**
     * Returns the JSON-serialized content payload for the java-canonical sugar dict.
     * Must be byte-identical to java-canonical.json header.content.
     */
    private String contentJson() {
        return "{"
            + "\"entries\":["
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @requires(${lhs} > ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"gt\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @requires(${lhs} >= ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"ge\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @requires(${lhs} < ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"lt\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @requires(${lhs} <= ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"le\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @requires(${lhs} == ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"eq\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @ensures(${lhs} > ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"ensures_gt\"}},"
            + "{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-method\",\"template\":\"// @ensures(${lhs} == ${rhs})\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"predicate_pattern\":{\"args\":[{\"args\":[],\"head\":\"var\",\"name\":\"${lhs}\"},{\"args\":[],\"head\":\"var\",\"name\":\"${rhs}\"}],\"head\":\"ensures_eq\"}}"
            + "],"
            + "\"sugar_name\":\"canonical\","
            + "\"target_language\":\"java\""
            + "}";
    }

    private String initResult() {
        return "{"
            + "\"name\":\"provekit-realize-java\","
            + "\"version\":\"0.1.0\","
            + "\"protocol_version\":\"provekit-orp/1\","
            + "\"capabilities\":{"
            + "\"kits\":[\"java\"],"
            + "\"modes\":[\"transform\"],"
            + "\"obligationKinds\":[\"gap\"],"
            + "\"predicates\":[\"non_null\"],"
            + "\"surfaces\":[\"java-provekit-native\",\"java-spring-web\"]"
            + "}"
            + "}";
    }

    private void sendResponse(String id, String result) {
        out.println("{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"result\":" + result + "}");
    }

    private void sendError(String id, int code, String message) {
        out.println("{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"error\":{\"code\":" + code + ",\"message\":" + JsonUtil.quoted(message) + "}}");
    }

    /**
     * Parse the JSON array string into a list of ParametricExpansion records.
     * Returns empty list on null / empty / parse failure (substrate-honest:
     * absent expansions just means no parametric CIDs to decompose).
     */
    /** Parse `{ "fn_name": "ret_type", ... }` into a java map. The map's
     *  values are RUST source-language type strings; SugarRealizer's
     *  mapSourceType translates them at lookup time. */
    private static java.util.Map<String, String> parseFunctionReturnTypes(String json) {
        if (json == null || json.isBlank() || "{}".equals(json.trim())) return java.util.Map.of();
        java.util.Map<String, String> out = new java.util.HashMap<>();
        try {
            com.provekit.ir.Jcs.Json parsed = com.provekit.ir.Jcs.parse(json);
            if (parsed instanceof com.provekit.ir.Jcs.Obj obj) {
                for (com.provekit.ir.Jcs.Field f : obj.fields()) {
                    if (f.value() instanceof com.provekit.ir.Jcs.Str s) {
                        out.put(f.key(), s.value());
                    }
                }
            }
        } catch (Exception ignore) {}
        return out;
    }

    private static java.util.List<SugarRealizer.ParametricExpansion> parseParametricExpansions(String json) {
        if (json == null || json.isBlank() || "[]".equals(json.trim())) return java.util.List.of();
        java.util.List<SugarRealizer.ParametricExpansion> out = new java.util.ArrayList<>();
        try {
            com.provekit.ir.Jcs.Json doc = com.provekit.ir.Jcs.parse(json);
            if (!(doc instanceof com.provekit.ir.Jcs.Arr arr)) return java.util.List.of();
            for (com.provekit.ir.Jcs.Json item : arr.values()) {
                if (!(item instanceof com.provekit.ir.Jcs.Obj o)) continue;
                String cid = o.stringFieldOrNull("cid");
                String ctor = o.stringFieldOrNull("constructor_cid");
                if (cid == null || ctor == null) continue;
                com.provekit.ir.Jcs.Json argsJson = o.get("arg_cids");
                java.util.List<String> argCids = new java.util.ArrayList<>();
                if (argsJson instanceof com.provekit.ir.Jcs.Arr argArr) {
                    for (com.provekit.ir.Jcs.Json a : argArr.values()) {
                        if (a instanceof com.provekit.ir.Jcs.Str s) argCids.add(s.value());
                    }
                }
                out.add(new SugarRealizer.ParametricExpansion(cid, ctor, argCids));
            }
        } catch (RuntimeException ignored) {
            // Substrate-honest: malformed expansion data → no decomposition possible.
        }
        return out;
    }
}
