// SPDX-License-Identifier: Apache-2.0

package com.provekit.realize;

import com.provekit.ir.Jcs;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.OutputStreamWriter;
import java.io.PrintWriter;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;

import static org.junit.jupiter.api.Assertions.*;

public class RealizationFragmentContractTest {
    @Test
    public void jacksonFragmentExposesImportsAndDependenciesStructurally() throws Exception {
        String response = invokeHandle("""
            {"jsonrpc":"2.0","id":11,"method":"provekit.plugin.invoke","params":{
              "function":"json_parse",
              "source_function_name":"json_parse",
              "params":["s"],
              "param_types":["java.lang.String"],
              "return_type":"JsonNode",
              "concept_name":"concept:json-parse",
              "target_library_tag":"jackson"
            }}
            """);

        Jcs.Obj doc = assertInstanceOf(Jcs.Obj.class, Jcs.parse(response));
        assertNull(doc.get("error"), "invoke must return a result, not error: " + response);
        Jcs.Obj result = doc.objectField("result");

        assertEquals("realization-fragment", result.stringField("kind"));
        assertFalse(
            result.stringField("source").contains("__fragment_imports__"),
            "fragment metadata must be structured, not smuggled in body comments"
        );

        Jcs.Arr imports = result.arrayField("imports");
        assertContains(imports, "com.fasterxml.jackson.databind.JsonNode");
        assertContains(imports, "com.fasterxml.jackson.databind.ObjectMapper");
        assertContains(imports, "com.fasterxml.jackson.core.JsonProcessingException");

        Jcs.Arr helpers = result.arrayField("helpers");
        assertContains(helpers, "static final ObjectMapper MAPPER = new ObjectMapper();");

        Jcs.Arr dependencies = result.arrayField("dependencies");
        assertContains(dependencies, "com.fasterxml.jackson.core:jackson-databind:2.17.2");
        assertContains(dependencies, "com.fasterxml.jackson.core:jackson-core:2.17.2");
        assertContains(dependencies, "com.fasterxml.jackson.core:jackson-annotations:2.17.2");

        String assembleResponse = invokeHandle("{\"jsonrpc\":\"2.0\",\"id\":12,"
            + "\"method\":\"provekit.plugin.assemble\",\"params\":{"
            + "\"target_lang\":\"java\","
            + "\"file_basename\":\"json_client\","
            + "\"fragments\":[" + Jcs.encode(result) + "]}}");
        Jcs.Obj assembleDoc = assertInstanceOf(Jcs.Obj.class, Jcs.parse(assembleResponse));
        assertNull(assembleDoc.get("error"), "assemble must return a result, not error: " + assembleResponse);
        Jcs.Obj assembleResult = assembleDoc.objectField("result");
        Jcs.Arr files = assembleResult.arrayField("files");
        Jcs.Obj file = assertInstanceOf(Jcs.Obj.class, files.values().get(0));
        String content = file.stringField("content");
        assertFalse(content.contains("__fragment_imports__"));
        assertTrue(content.indexOf("import com.fasterxml.jackson.databind.ObjectMapper;") < content.indexOf("public final class JsonClient"));
        assertTrue(content.indexOf("static final ObjectMapper MAPPER = new ObjectMapper();") < content.indexOf("public static JsonNode json_parse"));
        assertContains(assembleResult.arrayField("dependencies"), "com.fasterxml.jackson.core:jackson-databind:2.17.2");
    }

    private static void assertContains(Jcs.Arr array, String expected) {
        for (Jcs.Json value : array.values()) {
            if (value instanceof Jcs.Str s && s.value().equals(expected)) {
                return;
            }
        }
        fail("expected array to contain `" + expected + "` but got " + Jcs.encode(array));
    }

    private static String invokeHandle(String jsonLine) throws Exception {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        PrintWriter writer = new PrintWriter(new OutputStreamWriter(bytes, StandardCharsets.UTF_8), true);
        RpcServer server = new RpcServer();

        Field outField = RpcServer.class.getDeclaredField("out");
        outField.setAccessible(true);
        outField.set(server, writer);

        Method handle = RpcServer.class.getDeclaredMethod("handle", String.class);
        handle.setAccessible(true);
        handle.invoke(server, jsonLine);

        writer.flush();
        return bytes.toString(StandardCharsets.UTF_8).trim();
    }
}
