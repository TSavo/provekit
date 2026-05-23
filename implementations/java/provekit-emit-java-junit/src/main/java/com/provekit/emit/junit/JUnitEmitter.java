package com.provekit.emit.junit;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.provekit.ir.Blake3;
import com.provekit.ir.Jcs;

/**
 * Emits a JUnit5 test class from an {@link EmitPlan}.
 *
 * <p>The output is a self-contained java compilation unit: imports for
 * {@code org.junit.jupiter.api.Test} and a static import of the JUnit5
 * {@code Assertions}, a {@code <Function>ContractTest} class, and one
 * {@code @Test} method per supported predicate whose body asserts that
 * predicate via the inline {@link PredicateAssertionTable} mapping.
 *
 * <p>Substrate-honest: predicates this kit cannot spell are NOT emitted as
 * vacuously-passing tests. They are collected as {@code unsupported}
 * diagnostics so the substrate can record an honest "emit-assertion-gap"
 * per unhandled predicate.
 */
public final class JUnitEmitter {

    /** The emission result: source text, per-predicate gaps, and a CID. */
    public record Emission(
        String source,
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
            sb.append("\"kind\":\"junit-test-emission\"");
            sb.append(",\"source\":").append(jsonString(source));
            sb.append(",\"emitted_artifact_cid\":").append(jsonString(artifactCid));
            sb.append(",\"emitted_predicates\":").append(jsonStringArray(emittedPredicates));
            sb.append(",\"unsupported_predicates\":").append(jsonStringArray(unsupportedPredicates));
            sb.append(",\"is_complete\":").append(isComplete());
            return sb.append('}').toString();
        }
    }

    /** Emit a JUnit5 test class for the contract described by {@code plan}. */
    public Emission emit(EmitPlan plan) {
        String className = toPascalCase(plan.function()) + "ContractTest";

        List<String> emitted = new ArrayList<>();
        List<String> unsupported = new ArrayList<>();
        List<String> methods = new ArrayList<>();

        int idx = 0;
        for (Jcs.Obj predicate : plan.predicates()) {
            String head = PredicateAssertionTable.headOf(predicate);
            Optional<String> assertion = PredicateAssertionTable.render(predicate);
            if (assertion.isEmpty()) {
                unsupported.add(head == null ? "<malformed>" : head);
                continue;
            }
            emitted.add(head);
            methods.add(renderTestMethod(methodName(head, idx), assertion.get()));
            idx++;
        }

        String source = renderClass(className, methods);
        String cid = Blake3.blake3_512(source.getBytes(StandardCharsets.UTF_8));
        return new Emission(source, cid, emitted, unsupported);
    }

    private String renderClass(String className, List<String> methods) {
        StringBuilder sb = new StringBuilder();
        sb.append("import org.junit.jupiter.api.Test;\n");
        sb.append("import static org.junit.jupiter.api.Assertions.*;\n");
        sb.append('\n');
        sb.append("public class ").append(className).append(" {\n");
        for (int i = 0; i < methods.size(); i++) {
            if (i > 0) sb.append('\n');
            sb.append(methods.get(i));
        }
        sb.append("}\n");
        return sb.toString();
    }

    private String renderTestMethod(String methodName, String assertionStmt) {
        return "    @Test\n"
            + "    void " + methodName + "() {\n"
            + "        " + assertionStmt + "\n"
            + "    }\n";
    }

    private String methodName(String head, int idx) {
        // verifies<Head>[_<idx>] in camelCase; idx disambiguates repeats.
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

    /** PascalCase from a snake/camel/kebab function name. */
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
