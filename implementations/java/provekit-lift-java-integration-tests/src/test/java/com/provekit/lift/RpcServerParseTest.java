package com.provekit.lift;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

import java.io.*;
import java.nio.charset.StandardCharsets;

/**
 * Integration test: RpcServer parse method — daemon wire-protocol conformance.
 *
 * Verifies that the Java LSP plugin correctly handles the canonical NDJSON
 * "parse" method with {path, source} params and returns {declarations, callEdges}
 * per protocol/specs/2026-05-03-bridge-linkage-protocol.md §1 R1.
 *
 * We exercise the RpcServer directly (not as a subprocess) by swapping its
 * PrintWriter target and invoking handle() — the same pattern used by the
 * existing CrossDomainContractEquivalenceTest.
 */
public class RpcServerParseTest {

    // Java fixture with @NotNull annotations — the core "@NotNull → Scala IDE" demo.
    private static final String NOT_NULL_SOURCE = """
            import jakarta.validation.constraints.NotNull;
            public class UserService {
                public String greet(@NotNull String name) {
                    return "Hello " + name;
                }
                public void update(@NotNull String id, @NotNull String value) {
                }
            }
            """;

    // Source with embedded double-quotes in a string literal — exercises decodeJsonStringField.
    private static final String QUOTED_SOURCE = """
            import jakarta.validation.constraints.NotNull;
            public class QuoteService {
                public String label(@NotNull String msg) {
                    return "value: \\"" + msg + "\\"";
                }
            }
            """;

    /**
     * Invoke RpcServer.handle() with a parse request and capture the printed response.
     *
     * RpcServer.handle() is package-private. We reach it via a thin helper that
     * sets up the PrintWriter to point at a ByteArrayOutputStream rather than
     * System.out.
     */
    private String invokeHandle(String jsonLine) throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        PrintWriter pw = new PrintWriter(new OutputStreamWriter(baos, StandardCharsets.UTF_8), true);

        // Reflectively set the 'out' field so we capture output without spawning a process.
        RpcServer server = new RpcServer();
        java.lang.reflect.Field outField = RpcServer.class.getDeclaredField("out");
        outField.setAccessible(true);
        outField.set(server, pw);

        // Call the private handle() method.
        java.lang.reflect.Method handleMethod = RpcServer.class.getDeclaredMethod("handle", String.class);
        handleMethod.setAccessible(true);
        handleMethod.invoke(server, jsonLine);

        pw.flush();
        return baos.toString(StandardCharsets.UTF_8).trim();
    }

    @Test
    public void parseReturnsDeclarationsArray() throws Exception {
        // Build a JSON parse request. We encode the source as an escaped JSON string.
        String encodedSource = jsonEncodeString(NOT_NULL_SOURCE);
        String request = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"parse\",\"params\":{\"path\":\"/tmp/UserService.java\",\"source\":" + encodedSource + "}}";

        String response = invokeHandle(request);

        assertNotNull(response, "Response must not be null");
        assertTrue(response.contains("\"jsonrpc\":\"2.0\""), "Response must be JSON-RPC 2.0");
        assertTrue(response.contains("\"id\":1"), "Response id must match request id");
        assertTrue(response.contains("\"result\""), "Response must have result field");
        assertFalse(response.contains("\"error\""), "Response must not contain error");

        // The result must contain a declarations array.
        assertTrue(response.contains("\"declarations\""), "Result must contain declarations array");

        // The result must contain a callEdges array.
        assertTrue(response.contains("\"callEdges\""), "Result must contain callEdges array");

        // At least one declaration for 'greet' or 'update' must be present.
        // BeanValidationExtractor lifts @NotNull parameters as contract preconditions.
        assertTrue(
            response.contains("\"kind\":\"contract\""),
            "Declarations must contain at least one contract: " + response
        );
    }

    @Test
    public void parseSourceWithEmbeddedQuotes() throws Exception {
        // Verify that source containing Java string literals with \" is handled correctly.
        String encodedSource = jsonEncodeString(QUOTED_SOURCE);
        String request = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"parse\",\"params\":{\"path\":\"/tmp/QuoteService.java\",\"source\":" + encodedSource + "}}";

        String response = invokeHandle(request);

        assertNotNull(response);
        assertFalse(response.contains("\"error\""), "Must not error on source with embedded quotes: " + response);
        assertTrue(response.contains("\"declarations\""), "Must return declarations");
        // @NotNull on 'msg' parameter must lift.
        assertTrue(response.contains("\"kind\":\"contract\""), "Must lift @NotNull from quoted source: " + response);
    }

    @Test
    public void parseByteDeterminism() throws Exception {
        // Two identical requests must produce byte-for-byte identical responses.
        String encodedSource = jsonEncodeString(NOT_NULL_SOURCE);
        String request = "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"parse\",\"params\":{\"path\":\"/tmp/UserService.java\",\"source\":" + encodedSource + "}}";

        String response1 = invokeHandle(request);
        String response2 = invokeHandle(request);

        assertEquals(response1, response2, "parse responses must be byte-deterministic");
    }

    @Test
    public void parseCallEdgesIsArray() throws Exception {
        String encodedSource = jsonEncodeString(NOT_NULL_SOURCE);
        String request = "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"parse\",\"params\":{\"path\":\"/tmp/UserService.java\",\"source\":" + encodedSource + "}}";

        String response = invokeHandle(request);

        // callEdges must be a JSON array (may be empty — no JNI in fixture).
        int ceIdx = response.indexOf("\"callEdges\":");
        assertTrue(ceIdx >= 0, "Response must have callEdges key");
        int arrStart = response.indexOf('[', ceIdx);
        assertTrue(arrStart >= 0 && arrStart < ceIdx + 20, "callEdges value must be an array: " + response);
    }

    @Test
    public void initializeReturnsParseCapability() throws Exception {
        String request = "{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"initialize\",\"params\":{}}";
        String response = invokeHandle(request);

        assertTrue(response.contains("provekit-lsp-java"), "initialize result must name the plugin");
        assertTrue(response.contains("\"parse\""), "initialize capabilities must include 'parse'");
    }

    @Test
    public void parseEmptySourceReturnsEmptyArrays() throws Exception {
        String request = "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"parse\",\"params\":{\"path\":\"/tmp/Empty.java\",\"source\":\"\"}}";
        String response = invokeHandle(request);

        assertFalse(response.contains("\"error\""), "Empty source must not produce an error: " + response);
        assertTrue(response.contains("\"declarations\":[]"), "Empty source must return empty declarations");
        assertTrue(response.contains("\"callEdges\":[]"), "Empty source must return empty callEdges");
    }

    /**
     * Encode a Java string as a JSON string literal (with surrounding quotes).
     * Handles newlines, tabs, backslashes, and double-quotes.
     */
    static String jsonEncodeString(String s) {
        StringBuilder sb = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"'  -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default   -> sb.append(c);
            }
        }
        sb.append("\"");
        return sb.toString();
    }
}
