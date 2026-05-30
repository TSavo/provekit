package com.provekit.lift.java_source;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.provekit.ir.Jcs;
import org.junit.jupiter.api.Test;

class BindRpcServerTest {
    private static final String FLOOR_SOURCE = """
        // Forward-propagation floor fixture for Java
        // Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

        public class FloorFixture {
            public static boolean checkPositive(int x) {
                if (x <= 0) { return false; }  // pre: x > 0
                return true;
            }

            public static boolean callerSatisfiesPre() {
                boolean result = checkPositive(5);  // satisfies pre (x=5 > 0)
                return result;
            }

            public static boolean callerViolatesPre() {
                boolean result = checkPositive(-1);  // violates pre (x=-1 <= 0)
                return result;
            }

            public static boolean callerWithLoop() {
                for (int i = 0; i < 10; i++) {
                    boolean result = checkPositive(i);  // top fallback at loop entry
                    if (!result) { return false; }
                }
                return true;
            }
        }
        """;

    @Test
    void initializeAdvertisesSharedLspProtocol() {
        Jcs.Obj response = handle("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}");
        Jcs.Obj result = response.objectField("result");

        assertEquals("provekit-lsp-java", result.stringField("name"));
        assertEquals("provekit-lsp-shared/1", result.stringField("protocol_version"));
        assertEquals("java", result.stringField("kit_id"));
        Jcs.Obj capabilities = result.objectField("capabilities");
        assertTrue(Jcs.encode(capabilities.arrayField("source_surfaces")).contains("java-source"));
        assertTrue(Jcs.encode(capabilities.arrayField("diagnostic_codes")).contains("provekit.lsp.lift_gap"));
        assertTrue(Jcs.encode(capabilities.arrayField("diagnostic_codes")).contains("provekit.lsp.implication_failed"));
    }

    @Test
    void analyzeDocumentFloorFixtureEmitsSharedCallsiteDiagnostic() {
        String request = "{"
            + "\"jsonrpc\":\"2.0\","
            + "\"id\":2,"
            + "\"method\":\"analyzeDocument\","
            + "\"params\":{"
            + "\"kit_id\":\"java\","
            + "\"uri\":\"file:///project/FloorFixture.java\","
            + "\"file\":\"FloorFixture.java\","
            + "\"text\":" + jsonEncodeString(FLOOR_SOURCE) + ","
            + "\"document_version\":42,"
            + "\"workspace_root\":\"/project\","
            + "\"accepted_protocol_catalog_cids\":[],"
            + "\"policy_cids\":[]"
            + "}}";

        Jcs.Obj response = handle(request);
        Jcs.Obj result = response.objectField("result");

        assertEquals("lsp-document-analysis", result.stringField("kind"));
        assertEquals("1", result.stringField("schema_version"));
        assertEquals("java", result.stringField("kit_id"));
        assertEquals("file:///project/FloorFixture.java", result.stringField("uri"));
        assertEquals("FloorFixture.java", result.stringField("file"));
        String documentCid = result.stringField("document_cid");
        assertTrue(documentCid.startsWith("blake3-512:"));
        assertEquals("blake3-512:".length() + 128, documentCid.length());
        assertEquals(0, result.arrayField("statuses").values().size());
        assertTrue(result.get("project") instanceof Jcs.Null);

        Jcs.Arr diagnostics = result.arrayField("diagnostics");
        assertEquals(1, diagnostics.values().size());
        Jcs.Obj diagnostic = diagnostics.objectAt(0);
        assertEquals("provekit.lsp.implication_failed", diagnostic.stringField("code"));
        assertEquals("error", diagnostic.stringField("severity"));
        assertEquals("forward-propagation", diagnostic.stringField("producer"));
        assertEquals("java", diagnostic.stringField("kit_id"));
        Jcs.Obj range = diagnostic.objectField("range");
        assertEquals(16, ((Jcs.Num) range.get("start_line")).value());
        assertEquals(25, ((Jcs.Num) range.get("start_col")).value());
        assertEquals("checkPositive", diagnostic.objectField("data").stringField("callee"));
    }

    private static Jcs.Obj handle(String request) {
        return BindRpcServer.handle((Jcs.Obj) Jcs.parse(request));
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
