package com.provekit.lift;

import static org.junit.jupiter.api.Assertions.*;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

public class JavaImplicationLiftRpcTest {
    private static final String PARSE_CID =
        "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    private static final String NORMALIZE_CID =
        "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    @TempDir
    Path tempDir;

    @Test
    public void rpcLiftImplicationsEmitsBridgePerMatchedJavaCallExpression() throws Exception {
        Path src = write("src/App.java", """
            class App {
                int caller(String input) {
                    int parsed = parseInput(input);
                    return parsed.normalizeValue();
                }
            }
            """);

        String response = invokeRpc("""
            {"jsonrpc":"2.0","id":7,"method":"provekit.plugin.lift_implications","params":{
              "workspace_root":"%s",
              "source_paths":["%s"],
              "contract_bindings":[
                {"name":"parseInput@src/App.java:9:8","contract_cid":"%s"},
                {"name":"normalizeValue@src/App.java:12:8","contract_cid":"%s"}
              ]
            }}
            """.formatted(json(tempDir), json(tempDir.relativize(src)), PARSE_CID, NORMALIZE_CID));

        assertFalse(response.contains("\"error\""), response);
        assertTrue(response.contains("\"kind\":\"ir-document\""), response);
        assertTrue(response.contains("\"kind\":\"bridge\""), response);
        assertTrue(response.contains("\"sourceLayer\":\"java\""), response);
        assertTrue(response.contains("\"targetLayer\":\"java-tests\""), response);
        assertTrue(response.contains("\"sourceSymbol\":\"parseInput\""), response);
        assertTrue(response.contains("\"targetContractCid\":\"" + PARSE_CID + "\""), response);
        assertTrue(response.contains("\"target\":{\"cid\":\"" + PARSE_CID + "\",\"kind\":\"contract\"}"), response);
        assertTrue(response.contains("\"sourceSymbol\":\"normalizeValue\""), response);
        assertEquals(2, countOccurrences(response, "\"kind\":\"bridge\""), response);
    }

    @Test
    public void liftImplicationsScansSrcWhenSourcePathIsProjectRoot() throws Exception {
        write("src/App.java", """
            class App {
                int caller(String input) {
                    return parseInput(input);
                }
            }
            """);

        String response = invokeRpc("""
            {"jsonrpc":"2.0","id":8,"method":"provekit.plugin.lift_implications","params":{
              "workspace_root":"%s",
              "source_paths":["."],
              "contract_bindings":[{"name":"parseInput@src/App.java:9:8","contract_cid":"%s"}]
            }}
            """.formatted(json(tempDir), PARSE_CID));

        assertFalse(response.contains("\"error\""), response);
        assertTrue(response.contains("\"sourceSymbol\":\"parseInput\""), response);
        assertEquals(1, countOccurrences(response, "\"kind\":\"bridge\""), response);
    }

    @Test
    public void liftImplicationsEmitsLiftGapForUnmatchedCallee() throws Exception {
        Path src = write("src/App.java", """
            class App {
                int caller() {
                    return missingContract(0);
                }
            }
            """);

        String response = invokeRpc("""
            {"jsonrpc":"2.0","id":9,"method":"provekit.plugin.lift_implications","params":{
              "workspace_root":"%s",
              "source_paths":["%s"],
              "contract_bindings":[]
            }}
            """.formatted(json(tempDir), json(tempDir.relativize(src))));

        assertFalse(response.contains("\"error\""), response);
        assertTrue(response.contains("\"ir\":[]"), response);
        assertTrue(response.contains("\"kind\":\"lift-gap\""), response);
        assertTrue(response.contains("\"reason\":\"no-contract-for-callee\""), response);
        assertTrue(response.contains("\"callee\":\"missingContract\""), response);
    }

    private Path write(String relative, String source) throws Exception {
        Path path = tempDir.resolve(relative);
        Files.createDirectories(path.getParent());
        Files.writeString(path, source, StandardCharsets.UTF_8);
        return path;
    }

    private static String invokeRpc(String request) throws Exception {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        PrintStream original = System.out;
        try (PrintStream capture = new PrintStream(bytes, true, StandardCharsets.UTF_8)) {
            System.setOut(capture);
            RpcServer server = new RpcServer();
            Method handle = RpcServer.class.getDeclaredMethod("handle", String.class);
            handle.setAccessible(true);
            handle.invoke(server, request.replace("\n", ""));
        } finally {
            System.setOut(original);
        }
        return bytes.toString(StandardCharsets.UTF_8);
    }

    private static String json(Path path) {
        return json(path.toString());
    }

    private static String json(CharSequence value) {
        return value.toString().replace("\\", "\\\\").replace("\"", "\\\"");
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
