package com.provekit.lift.testng;

import static org.junit.jupiter.api.Assertions.*;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.github.javaparser.JavaParser;
import com.github.javaparser.ParseResult;
import com.github.javaparser.ast.CompilationUnit;
import com.provekit.lift.ContractDecl;
import com.provekit.lift.LiftHandler;

public class TestNgExtractorTest {
    @Test
    public void directAssertEqualsNamesContractByCallsite() {
        String source = """
            import org.testng.annotations.Test;
            import static org.testng.Assert.assertEquals;

            public class ParseTest {
                @Test
                public void testFoo() {
                    assertEquals(foo(5), 10);
                }
            }
            """;

        ContractDecl decl = liftOne(source);

        assertEquals(callsiteSymbol(source, "foo", "foo(5)"), decl.symbol);
        assertEquals(List.of(atom("eq", ctor("foo", cInt(5)), cInt(10))), decl.invariants);
        assertFalse(decl.symbol.contains("testFoo::"));
    }

    @Test
    public void variableBoundAssertTrueNamesContractByBindingCallsite() {
        String source = """
            import org.testng.annotations.Test;
            import static org.testng.Assert.assertTrue;

            public class ParseTest {
                @Test
                public void testFoo() {
                    int r = foo(5);
                    assertTrue(r > 0);
                }
            }
            """;

        ContractDecl decl = liftOne(source);

        assertEquals(callsiteSymbol(source, "foo", "foo(5)"), decl.symbol);
        assertEquals(
            List.of(atom("gt", ctor("foo", cInt(5)), cInt(0))),
            decl.invariants
        );
    }

    @Test
    public void literalAssertionWithoutCallsiteIsSkipped() {
        String source = """
            import org.testng.annotations.Test;
            import static org.testng.Assert.assertEquals;

            public class ParseTest {
                @Test
                public void testLiteral() {
                    assertEquals(10, 10);
                }
            }
            """;

        assertTrue(lift(source).isEmpty());
    }

    @Test
    public void liftHandlerLoadsTestNgExtractorThroughServiceLoader() {
        String source = """
            import org.testng.annotations.Test;
            import static org.testng.Assert.assertEquals;

            public class ParseTest {
                @Test
                public void testFoo() {
                    assertEquals(foo(5), 10);
                }
            }
            """;

        String result = new LiftHandler().parseSource("ParseTest.java", source);

        assertTrue(result.contains("\"symbol\":\"" + callsiteSymbol(source, "foo", "foo(5)") + "\""));
        assertTrue(result.contains("\"name\":\"eq\""));
        assertTrue(result.contains("\"name\":\"foo\""));
        assertFalse(result.contains("testFoo::"));
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
        return new TestNgExtractor().extract(result.getResult().get(), source);
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
