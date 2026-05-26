package com.provekit.lift.java_source;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.provekit.ir.Jcs;
import org.junit.jupiter.api.Test;

class BindRpcServerSharedLspTest {
    @Test
    void initializeAdvertisesSharedLspAnalyzeDocument() {
        Jcs.Obj request = Jcs.object(
            "id", Jcs.integer(1),
            "jsonrpc", Jcs.string("2.0"),
            "method", Jcs.string("initialize"),
            "params", Jcs.object("protocol_version", Jcs.string("provekit-lsp-shared/1"))
        );

        Jcs.Obj response = BindRpcServer.handle(request);
        Jcs.Obj result = response.objectField("result");
        Jcs.Obj capabilities = result.objectField("capabilities");

        assertEquals("provekit-lsp-shared/1", result.stringField("protocol_version"));
        assertEquals("java", result.stringField("kit_id"));
        assertArrayContains(capabilities.arrayField("methods"), "analyzeDocument");
        assertArrayContains(capabilities.arrayField("entry_kinds"), "library-sugar-binding-entry");
        assertArrayContains(capabilities.arrayField("status_kinds"), "materialize");
    }

    @Test
    void analyzeDocumentReturnsSharedShapeFromJavaOwnedLift() {
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:sql-query", library = "jdbc")
              String query(String id) {
                return "select * from users where id = " + id;
              }
            }
            """;
        Jcs.Obj request = Jcs.object(
            "id", Jcs.integer(2),
            "jsonrpc", Jcs.string("2.0"),
            "method", Jcs.string("analyzeDocument"),
            "params", Jcs.object(
                "file", Jcs.string("src/main/java/p/C.java"),
                "kit_id", Jcs.string("java"),
                "text", Jcs.string(source),
                "uri", Jcs.string("file:///project/src/main/java/p/C.java")
            )
        );

        Jcs.Obj response = BindRpcServer.handle(request);
        Jcs.Obj result = response.objectField("result");

        assertEquals("lsp-document-analysis", result.stringField("kind"));
        assertEquals("1", result.stringField("schema_version"));
        assertEquals("java", result.stringField("kit_id"));
        assertEquals("src/main/java/p/C.java", result.stringField("file"));
        assertTrue(result.stringField("document_cid").startsWith("blake3-512:"));

        Jcs.Arr entries = result.arrayField("entries");
        assertFalse(entries.isEmpty(), Jcs.encode(result));
        Jcs.Obj sugarEntry = firstEntryOfKind(entries, "library-sugar-binding-entry");
        assertNotNull(sugarEntry, Jcs.encode(result));
        assertEquals("kit", sugarEntry.stringField("producer"));
        assertEquals("java", sugarEntry.stringField("kit_id"));
        assertTrue(numberField(sugarEntry.objectField("range"), "start_line") > 0);
        assertEquals("library-sugar-binding-entry", sugarEntry.objectField("entry").stringField("kind"));

        Jcs.Arr statuses = result.arrayField("statuses");
        assertStatusPresent(statuses, "materialize");
        assertStatusPresent(statuses, "emit");
        assertStatusPresent(statuses, "check");
        assertStatusPresent(statuses, "prove");
    }

    @Test
    void analyzeDocumentRejectsWrongKitId() {
        Jcs.Obj request = Jcs.object(
            "id", Jcs.integer(3),
            "jsonrpc", Jcs.string("2.0"),
            "method", Jcs.string("analyzeDocument"),
            "params", Jcs.object(
                "file", Jcs.string("C.java"),
                "kit_id", Jcs.string("python"),
                "text", Jcs.string("class C {}"),
                "uri", Jcs.string("file:///C.java")
            )
        );

        Jcs.Obj response = BindRpcServer.handle(request);

        assertNotNull(response.get("error"), Jcs.encode(response));
        assertTrue(response.objectField("error").stringField("message").contains("kit_id"));
    }

    private static Jcs.Obj firstEntryOfKind(Jcs.Arr entries, String kind) {
        for (Jcs.Json entryJson : entries.values()) {
            if (entryJson instanceof Jcs.Obj entry && kind.equals(entry.stringFieldOrNull("kind"))) {
                return entry;
            }
        }
        return null;
    }

    private static void assertStatusPresent(Jcs.Arr statuses, String kind) {
        for (Jcs.Json statusJson : statuses.values()) {
            if (statusJson instanceof Jcs.Obj status && kind.equals(status.stringFieldOrNull("kind"))) {
                return;
            }
        }
        throw new AssertionError("missing status kind " + kind + ": " + Jcs.encode(statuses));
    }

    private static void assertArrayContains(Jcs.Arr arr, String expected) {
        for (Jcs.Json value : arr.values()) {
            if (value instanceof Jcs.Str s && expected.equals(s.value())) return;
        }
        throw new AssertionError("missing " + expected + " in " + Jcs.encode(arr));
    }

    private static long numberField(Jcs.Obj obj, String key) {
        Jcs.Json value = obj.get(key);
        if (value instanceof Jcs.Num n) return n.value();
        throw new AssertionError("field is not numeric: " + key + " in " + Jcs.encode(obj));
    }
}
