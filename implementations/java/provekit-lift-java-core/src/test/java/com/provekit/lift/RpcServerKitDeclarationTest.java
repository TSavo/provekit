package com.provekit.lift;

import static org.junit.jupiter.api.Assertions.*;

import com.provekit.ir.Jcs;
import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import org.junit.jupiter.api.Test;

class RpcServerKitDeclarationTest {
    @Test
    void kitDeclarationReturnsEmpiricalJavaCoreSurface() throws Exception {
        Jcs.Obj response = invoke(
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"provekit.plugin.kit_declaration\",\"params\":{}}"
        );
        assertNoRpcError(response);
        Jcs.Obj result = response.objectField("result");

        Jcs.Obj kit = result.objectField("kit");
        assertEquals("java", kit.stringField("id"));
        assertEquals("java", kit.stringField("language"));
        assertEquals("0.1.0", kit.stringField("version"));

        assertEquals("maven", result.objectField("proofResolution").stringField("strategy"));
        assertTrue(result.arrayField("effectKinds").isEmpty(), Jcs.encode(result));
        assertTrue(result.arrayField("effectLeaves").isEmpty(), Jcs.encode(result));
        assertTrue(result.arrayField("guardPredicates").isEmpty(), Jcs.encode(result));
        assertTrue(result.arrayField("controlCarriers").isEmpty(), Jcs.encode(result));
        assertTrue(result.arrayField("residueCategories").isEmpty(), Jcs.encode(result));

        assertEquals(7, result.objectField("rpc").arrayField("methods").values().size());
        assertMethodRequired(result, "initialize", true);
        assertMethodRequired(result, "provekit.plugin.kit_declaration", true);
        assertMethodRequired(result, "parse", true);
        assertMethodRequired(result, "provekit.plugin.lift_implications", true);
        assertMethodRequired(result, "lift", false);
        assertMethodRequired(result, "provekit.plugin.recognize", false);
        assertMethodRequired(result, "shutdown", false);
    }

    @Test
    void kitDeclarationResponseIsDeterministic() throws Exception {
        Jcs.Obj firstResponse = invoke(
            "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"provekit.plugin.kit_declaration\",\"params\":{}}"
        );
        Jcs.Obj secondResponse = invoke(
            "{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"provekit.plugin.kit_declaration\",\"params\":{}}"
        );
        assertNoRpcError(firstResponse);
        assertNoRpcError(secondResponse);

        assertEquals(
            Jcs.encode(firstResponse.objectField("result")),
            Jcs.encode(secondResponse.objectField("result"))
        );
    }

    @Test
    void initializeStaysSeparateFromKitDeclarationContent() throws Exception {
        Jcs.Obj response = invoke("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}");
        Jcs.Obj result = response.objectField("result");

        assertEquals("provekit-lsp-java", result.stringField("name"));
        assertEquals("0.1.0", result.stringField("version"));
        assertEquals(null, result.get("kit"));
        assertEquals(null, result.get("proofResolution"));
        assertEquals(null, result.get("effectKinds"));
        assertEquals(null, result.get("effectLeaves"));
    }

    private static Jcs.Obj invoke(String request) throws Exception {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        PrintStream original = System.out;
        try (PrintStream capture = new PrintStream(bytes, true, StandardCharsets.UTF_8)) {
            System.setOut(capture);
            RpcServer server = new RpcServer();
            Method handle = RpcServer.class.getDeclaredMethod("handle", String.class);
            handle.setAccessible(true);
            handle.invoke(server, request);
        } finally {
            System.setOut(original);
        }
        return (Jcs.Obj) Jcs.parse(bytes.toString(StandardCharsets.UTF_8).trim());
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
}
