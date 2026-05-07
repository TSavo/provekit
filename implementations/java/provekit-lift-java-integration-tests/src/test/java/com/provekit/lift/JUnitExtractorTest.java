package com.provekit.lift;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

import com.github.javaparser.JavaParser;
import com.github.javaparser.ParseResult;
import com.github.javaparser.ast.CompilationUnit;
import com.provekit.lift.junit.JUnitExtractor;

import java.util.List;

public class JUnitExtractorTest {

    @Test
    public void directAssertEqualsLiftsOnlyTheWitnessedCallTerm() {
        String source = """
            import org.junit.jupiter.api.Test;
            import static org.junit.jupiter.api.Assertions.assertEquals;

            public class ParseTest {
                @Test
                void parseFortyTwo() {
                    assertEquals(42, parseInt("42"));
                }
            }
            """;

        String json = liftOne(source).toJson();

        assertEquals(
            "{\"kind\":\"contract\",\"symbol\":\"parseFortyTwo::0\",\"invariant\":"
                + atom("eq", ctor("parseInt", cStr("42")), cInt(42))
                + "}",
            json
        );
        assertFalse(json.contains("parseInt\",\"args\":[{\"kind\":\"const\",\"value\":\"43\""),
            "A point assertion for parseInt(\"42\") must not mint a contract for parseInt(\"43\")");
        assertFalse(json.contains("\"symbol\":\"parseInt\""),
            "A point assertion must not become a universal function contract");
    }

    @Test
    public void assignedActualBindsAssertionThroughValueScopeImplication() {
        String source = """
            import org.junit.jupiter.api.Test;
            import static org.junit.jupiter.api.Assertions.assertEquals;

            public class ParseTest {
                @Test
                void parseFortyTwo() {
                    int actual = parseInt("42");
                    assertEquals(42, actual);
                }
            }
            """;

        String actual0 = var("actual$0");
        String expectedInvariant = implies(
            atom("eq", actual0, ctor("parseInt", cStr("42"))),
            atom("eq", actual0, cInt(42))
        );

        assertEquals(
            "{\"kind\":\"contract\",\"symbol\":\"parseFortyTwo::0\",\"invariant\":"
                + expectedInvariant
                + "}",
            liftOne(source).toJson()
        );
    }

    @Test
    public void reassignmentBindsAssertionThroughSsaStepLemmas() {
        String source = """
            import org.junit.jupiter.api.Test;
            import static org.junit.jupiter.api.Assertions.assertEquals;

            public class ParseTest {
                @Test
                void normalized() {
                    int actual = parseInt("42");
                    actual = normalize(actual);
                    assertEquals(42, actual);
                }
            }
            """;

        String actual0 = var("actual$0");
        String actual1 = var("actual$1");
        String expectedInvariant = implies(
            and(
                atom("eq", actual0, ctor("parseInt", cStr("42"))),
                atom("eq", actual1, ctor("normalize", actual0))
            ),
            atom("eq", actual1, cInt(42))
        );

        assertEquals(
            "{\"kind\":\"contract\",\"symbol\":\"normalized::0\",\"invariant\":"
                + expectedInvariant
                + "}",
            liftOne(source).toJson()
        );
    }

    @Test
    public void branchJoinExpandsAssertionAcrossGuardedValueScopes() {
        String source = """
            import org.junit.jupiter.api.Test;
            import org.junit.jupiter.api.Assertions;

            public class ParseTest {
                @Test
                void parsesBothRadices() {
                    int actual;
                    if (radix == 10) {
                        actual = parseInt("42");
                    } else {
                        actual = parseHex("2a");
                    }
                    Assertions.assertEquals(42, actual);
                }
            }
            """;

        String guard = atom("eq", var("radix"), cInt(10));
        String actual0 = var("actual$0");
        String actual1 = var("actual$1");
        String thenScope = and(
            guard,
            atom("eq", actual0, ctor("parseInt", cStr("42")))
        );
        String elseScope = and(
            not(guard),
            atom("eq", actual1, ctor("parseHex", cStr("2a")))
        );
        String expectedInvariant = and(
            implies(thenScope, atom("eq", actual0, cInt(42))),
            implies(elseScope, atom("eq", actual1, cInt(42)))
        );

        assertEquals(
            "{\"kind\":\"contract\",\"symbol\":\"parsesBothRadices::0\",\"invariant\":"
                + expectedInvariant
                + "}",
            liftOne(source).toJson()
        );
    }

    @Test
    public void unliftableBranchConditionFallsBackToOpaqueGuard() {
        String source = """
            import org.junit.jupiter.api.Test;
            import org.junit.jupiter.api.Assertions;

            public class ParseTest {
                @Test
                void parsesBothRadices() {
                    int actual;
                    if (radixes[0] == 10) {
                        actual = parseInt("42");
                    } else {
                        actual = parseHex("2a");
                    }
                    Assertions.assertEquals(42, actual);
                }
            }
            """;

        String guard = atom("junit_branch_condition", cStr("radixes[0] == 10"));
        String actual0 = var("actual$0");
        String actual1 = var("actual$1");
        String expectedInvariant = and(
            implies(
                and(guard, atom("eq", actual0, ctor("parseInt", cStr("42")))),
                atom("eq", actual0, cInt(42))
            ),
            implies(
                and(not(guard), atom("eq", actual1, ctor("parseHex", cStr("2a")))),
                atom("eq", actual1, cInt(42))
            )
        );

        assertEquals(
            "{\"kind\":\"contract\",\"symbol\":\"parsesBothRadices::0\",\"invariant\":"
                + expectedInvariant
                + "}",
            liftOne(source).toJson()
        );
    }

    private ContractDecl liftOne(String source) {
        List<ContractDecl> decls = lift(source);
        assertEquals(1, decls.size(), "Expected exactly one lifted assertion");
        return decls.get(0);
    }

    private List<ContractDecl> lift(String source) {
        ParseResult<CompilationUnit> result = new JavaParser().parse(source);
        assertTrue(result.isSuccessful() && result.getResult().isPresent(),
            "Failed to parse: " + result.getProblems());
        return new JUnitExtractor().extract(result.getResult().get(), source);
    }

    private static String var(String name) {
        return "{\"kind\":\"var\",\"name\":\"" + name + "\"}";
    }

    private static String cInt(long value) {
        return "{\"kind\":\"const\",\"value\":" + value
            + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";
    }

    private static String cStr(String value) {
        return "{\"kind\":\"const\",\"value\":\"" + value
            + "\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}";
    }

    private static String ctor(String name, String... args) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"ctor\",\"name\":\"")
            .append(name)
            .append("\",\"args\":[");
        for (int i = 0; i < args.length; i++) {
            if (i > 0) sb.append(",");
            sb.append(args[i]);
        }
        return sb.append("]}").toString();
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

    private static String and(String... operands) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"and\",\"operands\":[");
        for (int i = 0; i < operands.length; i++) {
            if (i > 0) sb.append(",");
            sb.append(operands[i]);
        }
        return sb.append("]}").toString();
    }

    private static String not(String operand) {
        return "{\"kind\":\"not\",\"operands\":[" + operand + "]}";
    }

    private static String implies(String antecedent, String consequent) {
        return "{\"kind\":\"implies\",\"operands\":["
            + antecedent
            + ","
            + consequent
            + "]}";
    }
}
