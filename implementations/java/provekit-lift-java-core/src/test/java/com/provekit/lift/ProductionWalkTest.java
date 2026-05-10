package com.provekit.lift;

import static org.junit.jupiter.api.Assertions.*;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.github.javaparser.JavaParser;
import com.github.javaparser.ParseResult;
import com.github.javaparser.ast.CompilationUnit;

public class ProductionWalkTest {
    @Test
    public void walkSubstitutesAssignmentBackToFunctionEntry() {
        String source = """
            public class App {
                static int checked(int x) {
                    if (x < 10) {
                        throw new IllegalArgumentException("x must be >= 10");
                    }
                    return x;
                }

                static int composedOk() {
                    int y = 42;
                    return checked(y);
                }
            }
            """;

        ProductionWalk.Result result = lift(source);

        assertEquals(3, result.declarations().size(), "callsite, let, and entry edges");
        assertEquals(3, result.implications().size(), "one implication per edge");
        assertTrue(result.implications().stream().allMatch(i -> i.prover.equals("java-wp-walk")));
        assertTrue(result.implications().stream().allMatch(i -> i.antecedentSlot.equals("pre")));
        assertTrue(result.implications().stream().allMatch(i -> i.consequentSlot.equals("post")));

        ContractDecl letEdge = find(result.declarations(), "::let:y");
        assertTrue(letEdge.symbol.startsWith("checked@App.java:"));
        assertEquals(List.of(atom("gte", cInt(42), cInt(10))), letEdge.preconditions);
        assertEquals(List.of(atom("gte", var("y"), cInt(10))), letEdge.postconditions);

        ContractDecl entryEdge = find(result.declarations(), "::entry");
        assertEquals(letEdge.preconditions, entryEdge.preconditions);
        assertEquals(letEdge.preconditions, entryEdge.postconditions);
    }

    @Test
    public void ifGuardBecomesCallsitePremise() {
        String source = """
            public class App {
                static int checked(int x) {
                    if (x < 10) {
                        throw new IllegalArgumentException();
                    }
                    return x;
                }

                static int guarded(int input) {
                    if (input >= 10) {
                        return checked(input);
                    }
                    return 0;
                }
            }
            """;

        ProductionWalk.Result result = lift(source);

        ContractDecl callsite = find(result.declarations(), "::callsite");
        String expected = implies(
            atom("gte", var("input"), cInt(10)),
            atom("gte", var("input"), cInt(10))
        );
        assertEquals(List.of(expected), callsite.preconditions);
        assertEquals(List.of(expected), callsite.postconditions);
    }

    private static ProductionWalk.Result lift(String source) {
        ParseResult<CompilationUnit> result = new JavaParser().parse(source);
        assertTrue(result.isSuccessful() && result.getResult().isPresent(),
            "Failed to parse: " + result.getProblems());
        return ProductionWalk.lift(result.getResult().get(), "App.java");
    }

    private static ContractDecl find(List<ContractDecl> decls, String suffix) {
        return decls.stream()
            .filter(d -> d.symbol.endsWith(suffix))
            .findFirst()
            .orElseThrow(() -> new AssertionError("missing edge suffix " + suffix + ": " + decls));
    }

    private static String var(String name) {
        return "{\"kind\":\"var\",\"name\":\"" + name + "\"}";
    }

    private static String cInt(long value) {
        return "{\"kind\":\"const\",\"value\":" + value
            + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";
    }

    private static String atom(String name, String... args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"atomic\",\"name\":\"")
            .append(name)
            .append("\",\"args\":[");
        for (int i = 0; i < args.length; i++) {
            if (i > 0) sb.append(",");
            sb.append(args[i]);
        }
        return sb.append("]}").toString();
    }

    private static String implies(String antecedent, String consequent) {
        return "{\"kind\":\"implies\",\"operands\":["
            + antecedent
            + ","
            + consequent
            + "]}";
    }
}
