package com.provekit.lift;

import static org.junit.jupiter.api.Assertions.*;

import java.nio.file.Files;
import java.nio.file.Path;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

public class LiftHandlerTest {
    @TempDir
    Path tempDir;

    @Test
    public void irDocumentLiftDeduplicatesFunctionContractsAcrossFiles() throws Exception {
        writeTwiceClass("A");
        writeTwiceClass("B");

        String ir = new LiftHandler().lift(tempDir.toString(), "java", "ir-document");

        assertEquals(
            1,
            countOccurrences(ir, "\"fn_name\":\"twice\""),
            "same body-derived function-contract symbol must be emitted once"
        );
    }

    @Test
    public void proofEnvelopeLiftOmitsVerifyFunctionContracts() throws Exception {
        writeTwiceClass("A");

        String ir = new LiftHandler().lift(tempDir.toString(), "java", "proof-envelope");

        assertEquals(
            0,
            countOccurrences(ir, "\"kind\":\"function-contract\""),
            "verify bridge function-contracts are gated to ir-document emit mode"
        );
    }

    @Test
    public void rpcExtractsNestedEmitMode() {
        String request = """
            {"jsonrpc":"2.0","id":2,"method":"lift","params":{"surface":"java","workspace_root":".","options":{"emit":"ir-document"}}}
            """;

        assertEquals("ir-document", RpcServer.extractEmitMode(request));
        assertEquals(
            "proof-envelope",
            RpcServer.extractEmitMode("{\"method\":\"lift\",\"params\":{\"surface\":\"java\"}}")
        );
    }

    private void writeTwiceClass(String className) throws Exception {
        Files.writeString(tempDir.resolve(className + ".java"), """
            public class A {
                static int twice(int x) {
                    return x * 2;
                }
            }
            """.replace("class A", "class " + className));
    }

    private static int countOccurrences(String haystack, String needle) {
        int count = 0;
        int offset = 0;
        while ((offset = haystack.indexOf(needle, offset)) >= 0) {
            count++;
            offset += needle.length();
        }
        return count;
    }
}
