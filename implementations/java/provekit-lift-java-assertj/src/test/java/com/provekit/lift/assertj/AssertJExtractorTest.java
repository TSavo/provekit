package com.provekit.lift.assertj;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.lang.reflect.Method;
import java.util.List;

import org.junit.jupiter.api.Test;

import com.github.javaparser.JavaParser;
import com.github.javaparser.ParseResult;
import com.github.javaparser.ast.CompilationUnit;
import com.provekit.lift.ContractDecl;

class AssertJExtractorTest {
    @Test
    void directAssertThatCallLiftsEqualityByProductionCallsite() {
        String source = """
            import org.junit.jupiter.api.Test;
            import static org.assertj.core.api.Assertions.assertThat;

            public class ParseTest {
                @Test
                void testFoo() {
                    assertThat(foo(5)).isEqualTo(10);
                }
            }
            """;

        ContractDecl decl = liftOne(source);

        assertEquals(callsiteSymbol(source, "foo", "foo(5)"), decl.symbol);
        assertEquals(List.of(atom("eq", ctor("foo", cInt(5)), cInt(10))), decl.invariants);
    }

    @Test
    void variableBoundAssertThatCallLiftsComparisonByBindingCallsite() {
        String source = """
            import org.junit.jupiter.api.Test;
            import static org.assertj.core.api.Assertions.assertThat;

            public class ParseTest {
                @Test
                void testFoo() {
                    int r = foo(5);
                    assertThat(r).isLessThan(10);
                }
            }
            """;

        ContractDecl decl = liftOne(source);

        assertEquals(callsiteSymbol(source, "foo", "foo(5)"), decl.symbol);
        assertEquals(List.of(atom("lt", ctor("foo", cInt(5)), cInt(10))), decl.invariants);
    }

    @Test
    void qualifiedAssertJAssertionsLiftNullnessAndInequality() {
        String source = """
            import org.junit.jupiter.api.Test;
            import org.assertj.core.api.Assertions;

            public class ParseTest {
                @Test
                void testFoo() {
                    Assertions.assertThat(name()).isNotNull();
                    Assertions.assertThat(count()).isNotEqualTo(0);
                }
            }
            """;

        List<ContractDecl> decls = lift(source);

        assertEquals(2, decls.size(), "Expected two lifted AssertJ assertions");
        assertEquals(callsiteSymbol(source, "name", "name()"), decls.get(0).symbol);
        assertEquals(List.of(atom("neq", ctor("name"), cNull())), decls.get(0).invariants);
        assertEquals(callsiteSymbol(source, "count", "count()"), decls.get(1).symbol);
        assertEquals(List.of(atom("neq", ctor("count"), cInt(0))), decls.get(1).invariants);
    }

    @Test
    void unsupportedAssertJFluentFormsDoNotLiftFakeContracts() {
        String source = """
            import org.junit.jupiter.api.Test;
            import static org.assertj.core.api.Assertions.assertThat;

            public class ParseTest {
                @Test
                void testFoo() {
                    assertThat(foo(5)).contains("5");
                }
            }
            """;

        assertTrue(lift(source).isEmpty());
    }

    private ContractDecl liftOne(String source) {
        List<ContractDecl> decls = lift(source);
        assertEquals(1, decls.size(), "Expected exactly one lifted AssertJ assertion");
        return decls.get(0);
    }

    @SuppressWarnings("unchecked")
    private List<ContractDecl> lift(String source) {
        ParseResult<CompilationUnit> result = new JavaParser().parse(source);
        assertTrue(result.isSuccessful() && result.getResult().isPresent(),
            "Failed to parse: " + result.getProblems());
        try {
            Class<?> extractorClass = Class.forName("com.provekit.lift.assertj.AssertJExtractor");
            Object extractor = extractorClass.getConstructor().newInstance();
            Method extract = extractorClass.getMethod("extract", CompilationUnit.class, String.class);
            return (List<ContractDecl>) extract.invoke(extractor, result.getResult().get(), source);
        } catch (ReflectiveOperationException e) {
            throw new AssertionError("AssertJ extractor is not available", e);
        }
    }

    private static String callsiteSymbol(String source, String callee, String snippet) {
        int offset = source.indexOf(snippet);
        assertTrue(offset >= 0, "Missing call snippet: " + snippet);
        int line = 1;
        int col = 1;
        for (int i = 0; i < offset; i++) {
            if (source.charAt(i) == '\n') {
                line++;
                col = 1;
            } else {
                col++;
            }
        }
        return callee + "@ParseTest.java:" + line + ":" + col;
    }

    private static String cInt(long value) {
        return "{\"kind\":\"const\",\"value\":" + value
            + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";
    }

    private static String cNull() {
        return "{\"kind\":\"const\",\"value\":null,\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"}}";
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
}
