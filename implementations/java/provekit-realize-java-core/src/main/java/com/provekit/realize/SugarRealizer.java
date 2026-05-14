package com.provekit.realize;

import com.provekit.ir.Jcs;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;
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
    record Realization(
            String source,
            boolean isStub,
            String observedLossRecord,
            String usedSugarsJson) {}

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
        return emitStub(function, params, paramTypes, returnType, conceptName, "", null);
    }

    static Realization emitStub(
            String function,
            List<String> params,
            List<String> paramTypes,
            String returnType,
            String conceptName,
            String mode,
            ContractPayload contract) {
        return emitStub(function, params, paramTypes, returnType, conceptName, mode, contract, List.of());
    }

    static Realization emitStub(
            String function,
            List<String> params,
            List<String> paramTypes,
            String returnType,
            String conceptName,
            String mode,
            ContractPayload contract,
            List<String> sugarPluginJson) {

        String className = snakeToPascal(function) + "Transported";
        String mappedReturn = mapSourceType(returnType);
        List<SugarEmission> sugarEmissions = SugarDictionary.emitAll(contract, sugarPluginJson);
        boolean hasBeanValidationNotNull = sugarEmissions.stream()
                .anyMatch(e -> e.surfaceLocator().startsWith("annotation:"));
        boolean hasJUnitWitness = sugarEmissions.stream()
                .anyMatch(e -> e.surfaceLocator().startsWith("witness:junit5"));

        StringBuilder typedParamList = new StringBuilder();
        for (int i = 0; i < params.size(); i++) {
            String name = params.get(i);
            String srcType = i < paramTypes.size() ? paramTypes.get(i) : "i64";
            String mapped = mapSourceType(srcType);
            if (i > 0) typedParamList.append(", ");
            if (hasBeanValidationNotNull && contractHasNonNullPrecondition(contract, name)) {
                typedParamList.append("@NotNull ");
            }
            typedParamList.append(mapped).append(" ").append(name);
        }

        // annotation_prefix for Java: top_indent = "    "
        String annotationPrefix = "    // concept: " + conceptName + "\n"
                + contractPrefix(contract)
                + commentPrefix(sugarEmissions);

        // Body: try body-template first, fall through to language stub.
        Optional<String> bodyTemplate = bodyTemplateFor(conceptName, params, mode);
        boolean isStub = bodyTemplate.isEmpty();
        String bodyContent = bodyTemplate
                .orElse("throw new UnsupportedOperationException(\"provekit-bind canonical: " + conceptName + "\");");
        // Multi-line templates: each internal line gets the same 8-space
        // method-body indent. Single-line bodies are unaffected (no \n).
        String indentedBody = bodyContent.replace("\n", "\n        ");
        String body = "        " + indentedBody + "\n";
        String methodAnnotation = hasBeanValidationNotNull && contractHasNonNullPostcondition(contract) ? "    @NotNull\n" : "";
        String imports = importsFor(hasBeanValidationNotNull, hasJUnitWitness);
        String witnessClass = hasJUnitWitness
                ? "\nfinal class " + className + "WitnessTest {\n"
                        + "    @Disabled(\"provekit witness skeleton requires concrete values\")\n"
                        + "    @Test\n"
                        + "    void contractWitnessesRemainRecoverable() {\n"
                        + junitWitnessBody(sugarEmissions)
                        + "    }\n"
                        + "}\n"
                : "";

        String source = imports
                + "final class " + className + " {\n"
                + annotationPrefix
                + methodAnnotation
                + "    public static " + mappedReturn + " " + function + "(" + typedParamList + ") {\n"
                + body
                + "    }\n"
                + "}\n"
                + witnessClass;
        return new Realization(source, isStub, observedLossRecordJson(sugarEmissions), usedSugarsJson(sugarEmissions));
    }

    private static String observedLossRecordJson(List<SugarEmission> emissions) {
        Map<String, Jcs.Json> byDimension = new TreeMap<>();
        for (SugarEmission emission : emissions) {
            if (emission.lossRecord() instanceof Jcs.Obj lossObj) {
                for (Jcs.Field field : lossObj.fields()) {
                    byDimension.merge(field.key(), field.value(), SugarRealizer::combineLossFormula);
                }
            }
        }
        List<Jcs.Field> fields = new ArrayList<>();
        for (Map.Entry<String, Jcs.Json> entry : byDimension.entrySet()) {
            fields.add(new Jcs.Field(entry.getKey(), entry.getValue()));
        }
        return Jcs.encode(new Jcs.Obj(fields));
    }

    private static Jcs.Json combineLossFormula(Jcs.Json existing, Jcs.Json next) {
        if (Jcs.encode(existing).equals(Jcs.encode(next))) {
            return existing;
        }
        return Jcs.object(
                "kind", Jcs.string("and"),
                "operands", Jcs.array(existing, next)
        );
    }

    private static String usedSugarsJson(List<SugarEmission> emissions) {
        List<Jcs.Json> used = new ArrayList<>();
        for (SugarEmission emission : emissions) {
            used.add(Jcs.object(
                    "cid", Jcs.string(emission.sugarCid()),
                    "loss_record", emission.lossRecord(),
                    "sugar_name", Jcs.string(emission.sugarName()),
                    "surface_locator", Jcs.string(emission.surfaceLocator())
            ));
        }
        return Jcs.encode(Jcs.array(used));
    }

    private static String importsFor(boolean hasBeanValidationNotNull, boolean hasJUnitWitness) {
        StringBuilder imports = new StringBuilder();
        if (hasBeanValidationNotNull) {
            imports.append("import jakarta.validation.constraints.NotNull;\n");
        }
        if (hasJUnitWitness) {
            imports.append("import org.junit.jupiter.api.Disabled;\n");
            imports.append("import org.junit.jupiter.api.Test;\n");
            imports.append("import static org.junit.jupiter.api.Assertions.assertNotNull;\n");
        }
        if (!imports.isEmpty()) {
            imports.append("\n");
        }
        return imports.toString();
    }

    private static String junitWitnessBody(List<SugarEmission> emissions) {
        StringBuilder out = new StringBuilder();
        Set<String> declared = new TreeSet<>();
        for (SugarEmission emission : emissions) {
            if (emission.surfaceLocator().startsWith("witness:junit5") && !emission.symbol().isBlank()) {
                String symbol = sanitizeJavaIdentifier(emission.symbol());
                if (declared.add(symbol)) {
                    out.append("        Object ").append(symbol).append(" = null;\n");
                }
            }
        }
        for (SugarEmission emission : emissions) {
            if (emission.surfaceLocator().startsWith("witness:junit5")) {
                out.append("        ").append(emission.rendered()).append("\n");
            }
        }
        return out.toString();
    }

    private static String sanitizeJavaIdentifier(String raw) {
        if (raw == null || raw.isBlank()) return "value";
        StringBuilder out = new StringBuilder();
        for (int i = 0; i < raw.length(); i++) {
            char ch = raw.charAt(i);
            boolean ok = i == 0 ? Character.isJavaIdentifierStart(ch) : Character.isJavaIdentifierPart(ch);
            out.append(ok ? ch : '_');
        }
        if (out.isEmpty() || !Character.isJavaIdentifierStart(out.charAt(0))) {
            return "value_" + out;
        }
        return out.toString();
    }

    private static String contractPrefix(ContractPayload contract) {
        if (contract == null || contract.localContractCid().isEmpty()) {
            return "";
        }
        StringBuilder out = new StringBuilder();
        out.append("    // contract-cid: ").append(contract.localContractCid()).append("\n");
        out.append("    // contract-mode: ").append(contract.dischargeVerdict()).append("\n");
        Set<String> kinds = new TreeSet<>();
        for (ContractWitness witness : contract.witnesses()) {
            if (!witness.sourceKind().isEmpty()) {
                kinds.add(witness.sourceKind());
            }
        }
        for (String kind : kinds) {
            out.append("    // contract-source: ").append(kind).append("\n");
        }
        return out.toString();
    }

    private static String commentPrefix(List<SugarEmission> emissions) {
        StringBuilder out = new StringBuilder();
        for (SugarEmission emission : emissions) {
            if (emission.surfaceLocator().startsWith("comment:")) {
                out.append("    ").append(emission.rendered()).append("\n");
            }
        }
        return out.toString();
    }

    private static boolean contractHasNonNullPrecondition(ContractPayload contract, String param) {
        if (contract == null) return false;
        for (ContractWitness witness : contract.witnesses()) {
            if ("pre".equals(witness.role())
                    && (isNonNullPredicateFor(witness.predicateText(), param)
                    || isNeqNullPredicateFor(witness.predicate(), param))) {
                return true;
            }
        }
        return false;
    }

    private static boolean contractHasNonNullPostcondition(ContractPayload contract) {
        if (contract == null) return false;
        for (ContractWitness witness : contract.witnesses()) {
            if ("post".equals(witness.role())
                    && (isNonNullPredicateFor(witness.predicateText(), "out")
                    || isNonNullPredicateFor(witness.predicateText(), "return")
                    || isNonNullPredicateFor(witness.predicateText(), "result")
                    || isNeqNullPredicateFor(witness.predicate(), "out")
                    || isNeqNullPredicateFor(witness.predicate(), "return")
                    || isNeqNullPredicateFor(witness.predicate(), "result"))) {
                return true;
            }
        }
        return false;
    }

    private static boolean isNonNullPredicateFor(String predicateText, String symbol) {
        String normalized = predicateText == null ? "" : predicateText.replaceAll("\\s+", "");
        return normalized.equals("non_null(" + symbol + ")")
                || normalized.equals(symbol + "!=null")
                || normalized.equals("not_null(" + symbol + ")");
    }

    private static boolean isNeqNullPredicateFor(Jcs.Json predicate, String symbol) {
        if (!(predicate instanceof Jcs.Obj obj)) return false;
        if (!"atomic".equals(obj.stringFieldOrNull("kind"))) return false;
        if (!"neq".equals(obj.stringFieldOrNull("name"))) return false;
        Jcs.Json argsJson = obj.get("args");
        if (!(argsJson instanceof Jcs.Arr args) || args.values().size() != 2) return false;
        return isVarNamed(args.get(0), symbol) && isConstNullTerm(args.get(1));
    }

    private static boolean isVarNamed(Jcs.Json term, String symbol) {
        return term instanceof Jcs.Obj obj
                && "var".equals(obj.stringFieldOrNull("kind"))
                && symbol.equals(obj.stringFieldOrNull("name"));
    }

    private static boolean isConstNullTerm(Jcs.Json term) {
        return term instanceof Jcs.Obj obj
                && "const".equals(obj.stringFieldOrNull("kind"))
                && obj.get("value") instanceof Jcs.Null;
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
            String mode,
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
        return bodyTemplateFor(conceptName, params, "");
    }

    static Optional<String> bodyTemplateFor(String conceptName, List<String> params, String mode) {
        List<BodyTemplateEntry> entries = entries();
        for (BodyTemplateEntry e : entries) {
            if (!conceptMatches(e.conceptName(), conceptName)) continue;
            if (!modeMatches(e.mode(), mode)) continue;
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

    private static boolean conceptMatches(String entryName, String requestName) {
        if (entryName.equals(requestName)) return true;
        if (entryName.startsWith("concept:") && entryName.substring("concept:".length()).equals(requestName)) {
            return true;
        }
        return requestName.startsWith("concept:")
                && requestName.substring("concept:".length()).equals(entryName);
    }

    private static boolean modeMatches(String entryMode, String requestMode) {
        if (entryMode == null || entryMode.isBlank()) return true;
        return requestMode != null && !requestMode.isBlank() && entryMode.equals(requestMode);
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
                String mode = itemObj.stringFieldOrNull("mode");

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
                out.add(new BodyTemplateEntry(conceptName, mode, kind, tmpl, minParams, maxParams));
            }
            return out;
        } catch (IOException e) {
            // I/O failure: degrade to "no entries"; stubs will emit.
            return List.of();
        }
    }
}

record SugarEmission(
        String sugarCid,
        String sugarName,
        String surfaceLocator,
        String rendered,
        String symbol,
        Jcs.Json lossRecord) {}

final class SugarDictionary {
    private SugarDictionary() {}

    static List<SugarEmission> emitAll(ContractPayload contract, List<String> pluginJson) {
        if (contract == null || pluginJson == null || pluginJson.isEmpty()) {
            return List.of();
        }
        List<SugarEmission> out = new ArrayList<>();
        for (String rawPlugin : pluginJson) {
            SugarPlugin plugin = SugarPlugin.fromJson(rawPlugin);
            if (plugin == null || !"java".equals(plugin.targetLanguage())) continue;
            for (ContractWitness witness : contract.witnesses()) {
                Jcs.Json predicate = witness.predicate();
                for (SugarEntry entry : plugin.entries()) {
                    Match match = match(predicate, entry.pattern(), witness);
                    if (match != null) {
                        out.add(new SugarEmission(
                                plugin.cid(),
                                plugin.sugarName(),
                                entry.surfaceLocator(),
                                render(entry.template(), match),
                                match.symbol(),
                                entry.lossRecord()
                        ));
                    }
                }
            }
        }
        return out;
    }

    private static Match match(Jcs.Json predicate, Jcs.Json pattern, ContractWitness witness) {
        if (!(pattern instanceof Jcs.Obj patternObj)) return null;
        String patternName = patternObj.stringFieldOrNull("name");
        if (patternName != null && patternName.startsWith("${")) {
            return new Match(witnessSymbol(witness), witness.predicateText(), witness.role());
        }
        if (!(predicate instanceof Jcs.Obj predicateObj)) return null;
        if (!"atomic".equals(predicateObj.stringFieldOrNull("kind"))) return null;
        if (patternName == null || !patternName.equals(predicateObj.stringFieldOrNull("name"))) return null;
        Jcs.Json patternArgsJson = patternObj.get("args");
        Jcs.Json predicateArgsJson = predicateObj.get("args");
        if (!(patternArgsJson instanceof Jcs.Arr patternArgs)
                || !(predicateArgsJson instanceof Jcs.Arr predicateArgs)
                || patternArgs.values().size() != predicateArgs.values().size()) {
            return null;
        }
        String symbol = "";
        for (int i = 0; i < patternArgs.values().size(); i++) {
            Jcs.Json p = patternArgs.get(i);
            Jcs.Json actual = predicateArgs.get(i);
            if (isHoleVar(p, "symbol") && actual instanceof Jcs.Obj actualObj) {
                symbol = actualObj.stringFieldOrNull("name");
                continue;
            }
            if (isConstNullPattern(p) && isConstNullPattern(actual)) {
                continue;
            }
            if (!Jcs.encode(p).equals(Jcs.encode(actual))) {
                return null;
            }
        }
        return new Match(symbol, witness.predicateText(), witness.role());
    }

    private static boolean isHoleVar(Jcs.Json json, String name) {
        return json instanceof Jcs.Obj obj
                && "var".equals(obj.stringFieldOrNull("kind"))
                && ("${" + name + "}").equals(obj.stringFieldOrNull("name"));
    }

    private static boolean isConstNullPattern(Jcs.Json json) {
        return json instanceof Jcs.Obj obj
                && "const".equals(obj.stringFieldOrNull("kind"))
                && obj.get("value") instanceof Jcs.Null;
    }

    private static String witnessSymbol(ContractWitness witness) {
        Jcs.Json predicate = witness.predicate();
        if (predicate instanceof Jcs.Obj obj && obj.get("args") instanceof Jcs.Arr args && !args.isEmpty()) {
            Jcs.Json first = args.get(0);
            if (first instanceof Jcs.Obj termObj && "var".equals(termObj.stringFieldOrNull("kind"))) {
                return termObj.stringFieldOrNull("name");
            }
        }
        return "value";
    }

    private static String render(String template, Match match) {
        return template
                .replace("${symbol}", match.symbol())
                .replace("${formula_pretty_print}", match.formulaPrettyPrint())
                .replace("${contract_role}", roleKeyword(match.role()));
    }

    private static String roleKeyword(String role) {
        return switch (role) {
            case "pre" -> "requires";
            case "post" -> "ensures";
            default -> "contract";
        };
    }

    private record Match(String symbol, String formulaPrettyPrint, String role) {}

    private record SugarPlugin(String cid, String sugarName, String targetLanguage, List<SugarEntry> entries) {
        static SugarPlugin fromJson(String raw) {
            try {
                Jcs.Json rootJson = Jcs.parse(raw);
                if (!(rootJson instanceof Jcs.Obj root)) return null;
                Jcs.Json headerJson = root.get("header");
                if (!(headerJson instanceof Jcs.Obj header)) return null;
                String cid = header.stringFieldOrNull("cid");
                Jcs.Json contentJson = header.get("content");
                if (!(contentJson instanceof Jcs.Obj content)) return null;
                String sugarName = content.stringFieldOrNull("sugar_name");
                String targetLanguage = content.stringFieldOrNull("target_language");
                Jcs.Json entriesJson = content.get("entries");
                if (!(entriesJson instanceof Jcs.Arr entriesArr)) return null;
                List<SugarEntry> entries = new ArrayList<>();
                for (Jcs.Json entryJson : entriesArr.values()) {
                    if (entryJson instanceof Jcs.Obj entryObj) {
                        SugarEntry entry = SugarEntry.fromJson(entryObj);
                        if (entry != null) entries.add(entry);
                    }
                }
                return new SugarPlugin(cid == null ? "" : cid, sugarName, targetLanguage, entries);
            } catch (IllegalArgumentException e) {
                return null;
            }
        }
    }

    private record SugarEntry(String surfaceLocator, String template, Jcs.Json pattern, Jcs.Json lossRecord) {
        static SugarEntry fromJson(Jcs.Obj entry) {
            Jcs.Json templateJson = entry.get("emission_template");
            if (!(templateJson instanceof Jcs.Obj templateObj)) return null;
            Jcs.Json lossJson = entry.get("loss_record_contribution");
            if (!(lossJson instanceof Jcs.Obj lossObj)) return null;
            Jcs.Json pattern = entry.get("predicate_pattern");
            if (pattern == null) return null;
            Jcs.Json value = lossObj.get("value");
            return new SugarEntry(
                    templateObj.stringFieldOrNull("surface_locator"),
                    templateObj.stringFieldOrNull("template"),
                    pattern,
                    value == null ? new Jcs.Obj(List.of()) : value
            );
        }
    }
}

record ContractWitness(String role, Jcs.Json predicate, String predicateText, String sourceKind) {
    ContractWitness(String role, String predicateText, String sourceKind) {
        this(role, predicateFromText(predicateText), predicateText, sourceKind);
    }

    ContractWitness {
        role = role == null ? "" : role;
        predicate = predicate == null ? new Jcs.Null() : predicate;
        predicateText = predicateText == null ? "" : predicateText;
        sourceKind = sourceKind == null ? "" : sourceKind;
    }

    private static Jcs.Json parsePredicate(String json, String fallbackText) {
        if (json != null && !json.isBlank()) {
            try {
                return Jcs.parse(json);
            } catch (IllegalArgumentException ignored) {
            }
        }
        return predicateFromText(fallbackText);
    }

    private static Jcs.Json predicateFromText(String text) {
        String normalized = text == null ? "" : text.replaceAll("\\s+", "");
        String symbol = symbolInside(normalized, "non_null");
        if (symbol.isEmpty()) symbol = symbolInside(normalized, "not_null");
        if (symbol.isEmpty() && normalized.endsWith("!=null")) {
            symbol = normalized.substring(0, normalized.length() - "!=null".length());
        }
        if (!symbol.isEmpty()) {
            return Jcs.object(
                    "args", Jcs.array(
                            Jcs.object("kind", Jcs.string("var"), "name", Jcs.string(symbol)),
                            Jcs.object(
                                    "kind", Jcs.string("const"),
                                    "sort", Jcs.object("kind", Jcs.string("primitive"), "name", Jcs.string("Ref")),
                                    "value", Jcs.nullValue()
                            )
                    ),
                    "kind", Jcs.string("atomic"),
                    "name", Jcs.string("neq")
            );
        }
        return new Jcs.Null();
    }

    private static String symbolInside(String text, String fn) {
        String prefix = fn + "(";
        return text.startsWith(prefix) && text.endsWith(")")
                ? text.substring(prefix.length(), text.length() - 1)
                : "";
    }

    static ContractWitness fromJson(String json) {
        String predicateJson = JsonUtil.extractObjectField(json, "predicate");
        String predicateText = JsonUtil.decodeJsonStringField(json, "predicate_text");
        return new ContractWitness(
                JsonUtil.decodeJsonStringField(json, "role"),
                parsePredicate(predicateJson, predicateText),
                firstNonEmpty(predicateText, predicateJson),
                JsonUtil.decodeJsonStringField(json, "source_kind")
        );
    }

    private static String firstNonEmpty(String first, String second) {
        return first == null || first.isEmpty() ? second : first;
    }
}

record ContractPayload(
        String conceptSiteCid,
        String localContractCid,
        String origin,
        String dischargeVerdict,
        List<ContractWitness> witnesses) {
    ContractPayload {
        conceptSiteCid = conceptSiteCid == null ? "" : conceptSiteCid;
        localContractCid = localContractCid == null ? "" : localContractCid;
        origin = origin == null ? "" : origin;
        dischargeVerdict = dischargeVerdict == null ? "" : dischargeVerdict;
        witnesses = witnesses == null ? List.of() : List.copyOf(witnesses);
    }

    static ContractPayload fromJson(String json) {
        if (json == null || json.isBlank() || "{}".equals(json.trim())) {
            return null;
        }
        List<ContractWitness> witnesses = JsonUtil.decodeJsonObjectArray(json, "witnesses")
                .stream()
                .map(ContractWitness::fromJson)
                .toList();
        return new ContractPayload(
                JsonUtil.decodeJsonStringField(json, "concept_site_cid"),
                JsonUtil.decodeJsonStringField(json, "local_contract_cid"),
                JsonUtil.decodeJsonStringField(json, "origin"),
                JsonUtil.decodeJsonStringField(json, "discharge_verdict"),
                witnesses
        );
    }
}
