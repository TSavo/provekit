package com.provekit.realize;

import com.provekit.ir.Jcs;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.stream.Collectors;

/**
 * Emits canonical Java method bodies and stubs for the bind canonical-rewrite path.
 *
 * Per the federation-by-construction directive ("all java emitter code belongs in java"),
 * this class is the SOLE owner of Java surface emission. cmd_transport.rs dispatches to
 * this class via the JSON-RPC plugin protocol; Rust holds no Java syntax.
 *
 * Output shape (per cmd_transport.rs Java branch, byte-identical):
 * <pre>
 * final class {PascalFn}Transported {
 *     // concept: {conceptName}
 *     public static {returnType} {function}({typedParams}) {
 *         {body}
 *     }
 * }
 * </pre>
 *
 * Where {body} is rendered from the body-template plugin (loaded from
 * classpath resource `com/provekit/realize/java-canonical-bodies.json`,
 * sourced from `menagerie/java-language-signature/specs/body-templates/`)
 * when an entry matches the binding's concept_name + signature_guard.
 * When no entry matches, the language stub falls through:
 *   `throw new UnsupportedOperationException("provekit-bind canonical: <concept>");`
 */
final class SugarRealizer {

    /**
     * Realization result: the emitted Java source plus a flag indicating
     * whether the body came from a body-template (real body, `is_stub=false`)
     * or fell through to the language stub (`is_stub=true`).
     *
     * Carried through to the JSON-RPC response so the caller (cmd_bind) can
     * emit accurate per-concept `bind-stub-body-emitted` gap entries per
     * `2026-05-13-body-template-memento.md` §5.
     */
    record Realization(String source, boolean isStub) {}

    /**
     * Emit a Java method for a single function. Body source comes from the
     * body-template plugin when a matching entry exists; otherwise an
     * idiomatic stub is emitted.
     *
     * @param function    Rust snake_case function name (e.g. "wrap_identity")
     * @param params      Parameter names in order (e.g. ["x"])
     * @param paramTypes  Source-language (Rust) type strings (e.g. ["i64"])
     * @param returnType  Source-language return type string (e.g. "i64")
     * @param conceptName Concept binding name (e.g. "identity")
     * @return Realization carrying source + is_stub flag.
     */
    static Realization emitStub(
            String function,
            List<String> params,
            List<String> paramTypes,
            String returnType,
            String conceptName) {

        String className = snakeToPascal(function) + "Transported";
        String mappedReturn = mapSourceType(returnType);

        StringBuilder typedParamList = new StringBuilder();
        for (int i = 0; i < params.size(); i++) {
            String name = params.get(i);
            String srcType = i < paramTypes.size() ? paramTypes.get(i) : "i64";
            String mapped = mapSourceType(srcType);
            if (i > 0) typedParamList.append(", ");
            typedParamList.append(mapped).append(" ").append(name);
        }

        // annotation_prefix for Java: top_indent = "    "
        String annotationPrefix = "    // concept: " + conceptName + "\n";

        // Body: try body-template first, fall through to language stub.
        Optional<String> bodyTemplate = bodyTemplateFor(conceptName, params);
        boolean isStub = bodyTemplate.isEmpty();
        String bodyContent = bodyTemplate
                .orElse("throw new UnsupportedOperationException(\"provekit-bind canonical: " + conceptName + "\");");
        String body = "        " + bodyContent + "\n";

        String source = "final class " + className + " {\n"
                + annotationPrefix
                + "    public static " + mappedReturn + " " + function + "(" + typedParamList + ") {\n"
                + body
                + "    }\n"
                + "}\n";
        return new Realization(source, isStub);
    }

    /**
     * Map a Rust source type to the Java equivalent.
     *
     * Mirrors cmd_transport.rs map_source_type for TargetStyle::Java.
     */
    static String mapSourceType(String src) {
        return switch (src) {
            case "()" -> "void";
            case "i64", "u64" -> "long";
            case "i32", "u32" -> "int";
            case "i16", "u16" -> "short";
            case "i8", "u8" -> "byte";
            case "f64" -> "double";
            case "f32" -> "float";
            case "bool" -> "boolean";
            case "String", "&str", "&String" -> "String";
            default -> src;
        };
    }

    /**
     * Convert snake_case to PascalCase.
     *
     * Mirrors cmd_transport.rs snake_to_pascal_local.
     */
    static String snakeToPascal(String s) {
        StringBuilder sb = new StringBuilder();
        for (String part : s.split("_", -1)) {
            if (part.isEmpty()) continue;
            sb.append(Character.toUpperCase(part.charAt(0)));
            if (part.length() > 1) sb.append(part.substring(1));
        }
        return sb.toString();
    }

    // -----------------------------------------------------------------------
    // Body-template loading per protocol/specs/2026-05-13-body-template-memento.md
    // -----------------------------------------------------------------------

    /**
     * Cached body-template entries, loaded lazily on first call from the
     * classpath resource `com/provekit/realize/java-canonical-bodies.json`.
     * The resource is sourced from
     * `menagerie/java-language-signature/specs/body-templates/java-canonical-bodies.json`
     * at build time (see pom.xml resource configuration).
     */
    private static volatile List<BodyTemplateEntry> ENTRIES_CACHE = null;

    /**
     * One body-template entry per spec §2.
     */
    private record BodyTemplateEntry(
            String conceptName,
            String templateKind,
            String template,
            Integer minParams,
            Integer maxParams) {}

    /**
     * Render a body string for the given concept + signature, or empty when
     * no entry matches (caller falls back to the language stub).
     *
     * Selection per spec §2.2: exact concept_name match + signature_guard
     * pass (min_params <= |params| <= max_params when present). Substitution
     * per §2.3: ${param0}, ${param1}, ... ${param_count}. Unbound placeholders
     * refuse-match.
     */
    static Optional<String> bodyTemplateFor(String conceptName, List<String> params) {
        List<BodyTemplateEntry> entries = entries();
        for (BodyTemplateEntry e : entries) {
            if (!e.conceptName().equals(conceptName)) continue;
            if (e.minParams() != null && params.size() < e.minParams()) continue;
            if (e.maxParams() != null && params.size() > e.maxParams()) continue;
            if (!"verbatim".equals(e.templateKind())) continue;
            String rendered = e.template();
            for (int i = 0; i < params.size(); i++) {
                rendered = rendered.replace("${param" + i + "}", params.get(i));
            }
            rendered = rendered.replace("${param_count}", Integer.toString(params.size()));
            if (rendered.contains("${")) {
                // Unbound placeholder: refuse-match per spec §2.1.
                continue;
            }
            return Optional.of(rendered);
        }
        return Optional.empty();
    }

    private static List<BodyTemplateEntry> entries() {
        List<BodyTemplateEntry> cached = ENTRIES_CACHE;
        if (cached != null) return cached;
        synchronized (SugarRealizer.class) {
            if (ENTRIES_CACHE != null) return ENTRIES_CACHE;
            ENTRIES_CACHE = loadEntriesFromResource();
            return ENTRIES_CACHE;
        }
    }

    private static List<BodyTemplateEntry> loadEntriesFromResource() {
        try (InputStream in = SugarRealizer.class.getResourceAsStream("java-canonical-bodies.json")) {
            if (in == null) {
                // Resource absent: degrade to "no entries" (callers get language stub).
                return List.of();
            }
            String raw;
            try (BufferedReader reader = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8))) {
                raw = reader.lines().collect(Collectors.joining("\n"));
            }
            Jcs.Json root = Jcs.parse(raw);
            if (!(root instanceof Jcs.Obj rootObj)) return List.of();
            Jcs.Json header = rootObj.get("header");
            if (!(header instanceof Jcs.Obj headerObj)) return List.of();
            Jcs.Json content = headerObj.get("content");
            if (!(content instanceof Jcs.Obj contentObj)) return List.of();
            Jcs.Json entriesJson = contentObj.get("entries");
            if (!(entriesJson instanceof Jcs.Arr entriesArr)) return List.of();

            List<BodyTemplateEntry> out = new ArrayList<>();
            for (Jcs.Json item : entriesArr.values()) {
                if (!(item instanceof Jcs.Obj itemObj)) continue;
                String conceptName = itemObj.stringFieldOrNull("concept_name");
                if (conceptName == null) continue;

                Jcs.Json template = itemObj.get("emission_template");
                if (!(template instanceof Jcs.Obj templateObj)) continue;
                String kind = templateObj.stringFieldOrNull("kind");
                String tmpl = templateObj.stringFieldOrNull("template");
                if (kind == null || tmpl == null) continue;

                Integer minParams = null;
                Integer maxParams = null;
                Jcs.Json guard = itemObj.get("signature_guard");
                if (guard instanceof Jcs.Obj guardObj) {
                    Jcs.Json minJ = guardObj.get("min_params");
                    Jcs.Json maxJ = guardObj.get("max_params");
                    if (minJ instanceof Jcs.Num minN) minParams = (int) minN.value();
                    if (maxJ instanceof Jcs.Num maxN) maxParams = (int) maxN.value();
                }
                out.add(new BodyTemplateEntry(conceptName, kind, tmpl, minParams, maxParams));
            }
            return out;
        } catch (IOException e) {
            // I/O failure: degrade to "no entries"; stubs will emit.
            return List.of();
        }
    }
}
