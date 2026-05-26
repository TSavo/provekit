package com.provekit.lift;

import static org.junit.jupiter.api.Assertions.*;

import java.util.List;
import java.util.stream.Collectors;

import org.junit.jupiter.api.Test;

import com.github.javaparser.JavaParser;
import com.github.javaparser.ParseResult;
import com.github.javaparser.ast.CompilationUnit;

public class VerifyLiftTest {
    @Test
    public void liftsBodyContractAndAssertCallsiteForVerifyBridge() {
        String source = """
            public class App {
                static int twice(int x) {
                    return x * 2;
                }

                static void check() {
                    assert twice(3) == 6;
                }
            }
            """;

        List<ContractDecl> decls = lift(source);

        assertEquals(2, decls.size(), "function-contract plus one assertion contract");
        String json = decls.stream()
            .map(ContractDecl::toJson)
            .collect(Collectors.joining("\n"));
        assertTrue(json.contains("\"kind\":\"function-contract\""), json);
        assertTrue(json.contains("\"fn_name\":\"twice\""), json);
        assertTrue(json.contains("\"bridgeSourceSymbol\":\"twice\""), json);
        assertTrue(json.contains("\"formals\":[\"x\"]"), json);
        assertTrue(json.contains("\"post\":{\"kind\":\"atomic\",\"name\":\"=\""), json);
        assertTrue(json.contains("\"name\":\"*\""), json);
        assertTrue(json.contains("\"symbol\":\"twice@App.java:"), json);
        assertTrue(json.contains("\"inv\":{\"kind\":\"atomic\",\"name\":\"=\""), json);
        assertTrue(json.contains("\"name\":\"twice\""), json);
    }

    private static List<ContractDecl> lift(String source) {
        ParseResult<CompilationUnit> result = new JavaParser().parse(source);
        assertTrue(result.isSuccessful() && result.getResult().isPresent(),
            "Failed to parse: " + result.getProblems());
        return VerifyLift.lift(result.getResult().get(), "App.java");
    }
}
