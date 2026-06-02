package com.provekit.lift.java_source;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.provekit.ir.Jcs;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class SourceRpcServerTest {
    @TempDir
    Path temp;

    @Test
    void initializeAdvertisesJavaSourceLiftProtocol() throws Exception {
        Jcs.Obj response = handle("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}");
        Jcs.Obj result = response.objectField("result");

        assertEquals("provekit-lift-java-source", result.stringField("name"));
        assertEquals("pep/1.7.0", result.stringField("protocol_version"));
        Jcs.Obj capabilities = result.objectField("capabilities");
        assertTrue(Jcs.encode(capabilities.arrayField("authoring_surfaces")).contains("java-source"));
        assertEquals("v1.1.0", capabilities.stringField("ir_version"));
    }

    @Test
    void kitDeclarationReturnsEmpiricalJavaSourceSurface() throws Exception {
        Jcs.Obj response = handle(
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"provekit.plugin.kit_declaration\",\"params\":{}}"
        );
        assertNoRpcError(response);
        Jcs.Obj result = response.objectField("result");

        Jcs.Obj kit = result.objectField("kit");
        assertEquals("java-source", kit.stringField("id"));
        assertEquals("java", kit.stringField("language"));
        assertEquals("0.1.0", kit.stringField("version"));

        assertEquals("maven", result.objectField("proofResolution").stringField("strategy"));
        assertEquals("concept:panic-freedom", result.arrayField("effectKinds").stringAt(0).value());

        Jcs.Arr effectLeaves = result.arrayField("effectLeaves");
        assertEquals(1, effectLeaves.values().size(), Jcs.encode(result));
        Jcs.Obj leaf = effectLeaves.objectAt(0);
        assertEquals("java-source", leaf.stringField("surface"));
        assertEquals("java:throw", leaf.stringField("local"));
        assertEquals("concept:panic-freedom.leaf.runtime-failure-site", leaf.stringField("concept"));

        assertTrue(result.arrayField("guardPredicates").isEmpty());
        assertTrue(result.arrayField("controlCarriers").isEmpty());
        assertTrue(result.arrayField("residueCategories").isEmpty());

        assertMethodRequired(result, "initialize", true);
        assertMethodRequired(result, "provekit.plugin.kit_declaration", true);
        assertMethodRequired(result, "lift", true);
        assertMethodRequired(result, "shutdown", false);
    }

    @Test
    void kitDeclarationResponseIsDeterministic() throws Exception {
        Jcs.Obj firstResponse = handle(
            "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"provekit.plugin.kit_declaration\",\"params\":{}}"
        );
        Jcs.Obj secondResponse = handle(
            "{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"provekit.plugin.kit_declaration\",\"params\":{}}"
        );
        assertNoRpcError(firstResponse);
        assertNoRpcError(secondResponse);
        Jcs.Obj first = firstResponse.objectField("result");
        Jcs.Obj second = secondResponse.objectField("result");

        assertEquals(Jcs.encode(first), Jcs.encode(second));
    }

    @Test
    void initializeStaysSeparateFromKitDeclarationContent() throws Exception {
        Jcs.Obj response = handle("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}");
        Jcs.Obj result = response.objectField("result");

        assertEquals(null, result.get("kit"));
        assertEquals(null, result.get("effectKinds"));
        assertEquals(null, result.get("effectLeaves"));
    }

    @Test
    void liftReturnsFunctionContractWithExplicitThrowRuntimeFailureLocus() throws Exception {
        Files.writeString(temp.resolve("Thrower.java"), """
public class Thrower {
static int fail(int x) {
if (x < 0) {
throw new IllegalStateException("neg");
}
return x;
}
}
""");

        String request = "{"
            + "\"jsonrpc\":\"2.0\","
            + "\"id\":2,"
            + "\"method\":\"lift\","
            + "\"params\":{"
            + "\"surface\":\"java-source\","
            + "\"workspace_root\":" + jsonEncodeString(temp.toString()) + ","
            + "\"source_paths\":[\"Thrower.java\"],"
            + "\"options\":{\"layer\":\"all\",\"identifyOnly\":false}"
            + "}}";

        Jcs.Obj response = handle(request);
        Jcs.Obj result = response.objectField("result");

        assertEquals("ir-document", result.stringField("kind"));
        Jcs.Obj contract = contractByName(result, "Thrower.fail(int)");
        Jcs.Arr loci = contract.arrayField("panicLoci");
        assertEquals(1, loci.values().size(), Jcs.encode(contract));
        Jcs.Obj locus = loci.objectAt(0);
        assertEquals("concept:panic-freedom", locus.stringField("effectKind"));
        assertEquals("concept:panic-freedom.leaf.runtime-failure-site", locus.stringField("callee"));
        assertEquals("explicit-throw", locus.stringField("subkind"));
        assertEquals("IllegalStateException", locus.stringField("exceptionClass"));
        assertEquals("Thrower.java", locus.stringField("file"));
        assertEquals(4, ((Jcs.Num) locus.get("line")).value());
        assertEquals(1, ((Jcs.Num) locus.get("col")).value());
        assertEquals("java:new", locus.objectField("argTerm").stringField("name"));
    }

    @Test
    void shutdownReturnsNullResult() throws Exception {
        Jcs.Obj response = handle("{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"shutdown\",\"params\":{}}");

        assertTrue(response.get("result") instanceof Jcs.Null, Jcs.encode(response));
    }

    private static Jcs.Obj handle(String request) throws Exception {
        return SourceRpcServer.handle((Jcs.Obj) Jcs.parse(request));
    }

    private static Jcs.Obj contractByName(Jcs.Obj result, String fnName) {
        return result.arrayField("ir").values().stream()
            .map(Jcs.Obj.class::cast)
            .filter(o -> fnName.equals(o.stringFieldOrNull("fnName")))
            .findFirst()
            .orElseThrow();
    }

    private static void assertNoRpcError(Jcs.Obj response) {
        assertEquals(null, response.get("error"), Jcs.encode(response));
    }

    private static void assertMethodRequired(Jcs.Obj declaration, String name, boolean required) {
        Jcs.Arr methods = declaration.objectField("rpc").arrayField("methods");
        Jcs.Obj method = methods.values().stream()
            .map(Jcs.Obj.class::cast)
            .filter(candidate -> name.equals(candidate.stringFieldOrNull("name")))
            .findFirst()
            .orElseThrow();
        assertEquals(required, method.boolField("required"), Jcs.encode(method));
    }

    private static String jsonEncodeString(String s) {
        StringBuilder sb = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> sb.append(c);
            }
        }
        sb.append("\"");
        return sb.toString();
    }
}
