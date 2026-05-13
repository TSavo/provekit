package com.provekit.lift.junit;

import static org.junit.jupiter.api.Assertions.*;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.github.javaparser.JavaParser;
import com.github.javaparser.ParseResult;
import com.github.javaparser.ast.CompilationUnit;
import com.provekit.lift.ContractDecl;

public class JUnitExtractorTest {
    @Test
    public void directCallAssertionNamesContractByCallsite() {
        String source = """
            import org.junit.jupiter.api.Test;
            import static org.junit.jupiter.api.Assertions.assertEquals;

            public class ParseTest {
                @Test
                void testFoo() {
                    assertEquals(10, foo(5));
                }
            }
            """;

        ContractDecl decl = liftOne(source);

        assertEquals(callsiteSymbol(source, "foo", "foo(5)"), decl.symbol);
        assertEquals(List.of(atom("eq", ctor("foo", cInt(5)), cInt(10))), decl.invariants);
        assertFalse(decl.symbol.contains("testFoo::"));
    }

    @Test
    public void variableBoundCallAssertionNamesContractByBindingCallsite() {
        String source = """
            import org.junit.jupiter.api.Test;
            import static org.junit.jupiter.api.Assertions.assertEquals;

            public class ParseTest {
                @Test
                void testFoo() {
                    int r = foo(5);
                    assertEquals(10, r);
                }
            }
            """;

        ContractDecl decl = liftOne(source);

        assertEquals(callsiteSymbol(source, "foo", "foo(5)"), decl.symbol);
        assertEquals(List.of(atom("eq", ctor("foo", cInt(5)), cInt(10))), decl.invariants);
        assertFalse(decl.toJson().contains("testFoo::"));
    }

    @Test
    public void literalAssertionWithoutCallsiteIsSkipped() {
        String source = """
            import org.junit.jupiter.api.Test;
            import static org.junit.jupiter.api.Assertions.assertEquals;

            public class ParseTest {
                @Test
                void testLiteral() {
                    assertEquals(10, 10);
                }
            }
            """;

        assertTrue(lift(source).isEmpty());
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
