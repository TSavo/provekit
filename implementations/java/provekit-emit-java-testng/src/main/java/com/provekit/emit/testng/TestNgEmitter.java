package com.provekit.emit.testng;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;

import com.provekit.ir.Blake3;
import com.provekit.ir.Jcs;

/** Emits a native TestNG test class from an {@link EmitPlan}. */
public final class TestNgEmitter {

    /** The emission result: source text, per-predicate gaps, and a CID. */
    public record Emission(
        String source,
        String path,
        String artifactCid,
        List<String> emittedPredicates,
        List<String> unsupportedPredicates
    ) {
        public Emission {
            emittedPredicates = List.copyOf(emittedPredicates);
            unsupportedPredicates = List.copyOf(unsupportedPredicates);
        }

        public boolean isComplete() {
            return unsupportedPredicates.isEmpty() && !emittedPredicates.isEmpty();
        }

        public String toJson() {
            StringBuilder sb = new StringBuilder("{");
            sb.append("\"kind\":\"testng-test-emission\"");
            sb.append(",\"source\":").append(jsonString(source));
            sb.append(",\"path\":").append(jsonString(path));
            sb.append(",\"extension\":\"java\"");
            sb.append(",\"emitted_artifact_cid\":").append(jsonString(artifactCid));
            sb.append(",\"emitted_predicates\":").append(jsonStringArray(emittedPredicates));
            sb.append(",\"unsupported_predicates\":").append(jsonStringArray(unsupportedPredicates));
            sb.append(",\"is_complete\":").append(isComplete());
            return sb.append('}').toString();
        }
    }

    /** Emit a TestNG test class for the contract described by {@code plan}. */
    public Emission emit(EmitPlan plan) {
        String className = toPascalCase(plan.function()) + "ContractTest";

        Map<String, String> declaredTypes = new LinkedHashMap<>();
        List<String> formals = plan.params();
        List<String> formalTypes = plan.paramTypes();
        for (int i = 0; i < formals.size(); i++) {
            String t = i < formalTypes.size() ? formalTypes.get(i) : "int";
            declaredTypes.put(formals.get(i), t == null || t.isBlank() ? "int" : t);
        }

        List<String> emitted = new ArrayList<>();
        List<String> unsupported = new ArrayList<>();
        List<String> methods = new ArrayList<>();

        int idx = 0;
        for (Jcs.Obj predicate : plan.predicates()) {
            String head = PredicateAssertionTable.headOf(predicate);
            Optional<String> assertion = PredicateAssertionTable.render(predicate);
            if (assertion.isEmpty()) {
                unsupported.add(rawHead(predicate));
                continue;
            }
            emitted.add(head);
            List<String> declarations = freeVarDeclarations(predicate, head, declaredTypes);
            methods.add(renderTestMethod(methodName(head, idx), declarations, assertion.get()));
            idx++;
        }

        String source = renderClass(className, methods);
        String cid = Blake3.blake3_512(source.getBytes(StandardCharsets.UTF_8));
        return new Emission(source, className + ".java", cid, emitted, unsupported);
    }

    private List<String> freeVarDeclarations(
            Jcs.Obj predicate, String head, Map<String, String> declaredTypes) {
        Set<String> vars = new LinkedHashSet<>();
        collectVars(predicate, vars);
        String fallbackType = defaultTypeFor(head);
        List<String> decls = new ArrayList<>();
        for (String v : vars) {
            String type = declaredTypes.getOrDefault(v, fallbackType);
            decls.add(type + " " + v + " = " + defaultValueFor(type) + ";");
        }
        return decls;
    }

    private static void collectVars(Jcs.Json term, Set<String> out) {
        if (!(term instanceof Jcs.Obj obj)) return;
        String kind = obj.stringFieldOrNull("kind");
        if ("var".equals(kind)) {
            String name = obj.stringFieldOrNull("name");
            if (name != null && !name.isBlank()) out.add(name);
            return;
        }
        Jcs.Json args = obj.get("args");
        if (args instanceof Jcs.Arr arr) {
            for (Jcs.Json a : arr.values()) collectVars(a, out);
        }
    }

    private static String defaultTypeFor(String head) {
        if (head == null) return "int";
        switch (head) {
            case "option-is-some":
            case "option-is-none":
            case "not-null":
            case "fallible-err":
                return "Object";
            default:
                return "int";
        }
    }

    private static String defaultValueFor(String type) {
        switch (type) {
            case "int":
            case "long":
            case "short":
            case "byte":
                return "0";
            case "double":
            case "float":
                return "0.0";
            case "boolean":
                return "false";
            case "char":
                return "'\\0'";
            default:
                return "null";
        }
    }

    private String renderClass(String className, List<String> methods) {
        StringBuilder sb = new StringBuilder();
        sb.append("import org.testng.annotations.Test;\n");
        sb.append("import org.testng.Assert;\n");
        sb.append('\n');
        sb.append("public class ").append(className).append(" {\n");
        for (int i = 0; i < methods.size(); i++) {
            if (i > 0) sb.append('\n');
            sb.append(methods.get(i));
        }
        sb.append("}\n");
        return sb.toString();
    }

    private String renderTestMethod(
            String methodName, List<String> declarations, String assertionStmt) {
        StringBuilder sb = new StringBuilder();
        sb.append("    @Test\n");
        sb.append("    public void ").append(methodName).append("() {\n");
        for (String decl : declarations) {
            sb.append("        ").append(decl).append('\n');
        }
        sb.append("        ").append(assertionStmt).append('\n');
        sb.append("    }\n");
        return sb.toString();
    }

    private String methodName(String head, int idx) {
        String safe = head == null ? "predicate" : head.replace('-', '_');
        StringBuilder sb = new StringBuilder("verifies");
        boolean up = true;
        for (int i = 0; i < safe.length(); i++) {
            char c = safe.charAt(i);
            if (c == '_') {
                up = true;
            } else if (up) {
                sb.append(Character.toUpperCase(c));
                up = false;
            } else {
                sb.append(c);
            }
        }
        sb.append('_').append(idx);
        return sb.toString();
    }

    static String toPascalCase(String name) {
        if (name == null || name.isBlank()) return "Contract";
        StringBuilder sb = new StringBuilder();
        boolean up = true;
        for (int i = 0; i < name.length(); i++) {
            char c = name.charAt(i);
            if (c == '_' || c == '-' || c == '.') {
                up = true;
            } else if (up) {
                sb.append(Character.toUpperCase(c));
                up = false;
            } else {
                sb.append(c);
            }
        }
        return sb.length() == 0 ? "Contract" : sb.toString();
    }

    private static String rawHead(Jcs.Obj predicate) {
        String name = predicate.stringFieldOrNull("name");
        if (name == null || name.isBlank()) name = predicate.stringFieldOrNull("predicate");
        if (name == null || name.isBlank()) return "<malformed>";
        return name.startsWith("concept:") ? name.substring("concept:".length()) : name;
    }

    private static String jsonString(String s) {
        if (s == null) return "null";
        StringBuilder sb = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> {
                    if (c < 0x20) sb.append(String.format("\\u%04x", (int) c));
                    else sb.append(c);
                }
            }
        }
        return sb.append('"').toString();
    }

    private static String jsonStringArray(List<String> items) {
        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < items.size(); i++) {
            if (i > 0) sb.append(',');
            sb.append(jsonString(items.get(i)));
        }
        return sb.append(']').toString();
    }
}
