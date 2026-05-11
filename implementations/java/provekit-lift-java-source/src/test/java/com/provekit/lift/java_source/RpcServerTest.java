package com.provekit.lift.java_source;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.provekit.ir.Jcs;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;

class RpcServerTest {
    @Test
    void initializeReportsDraftJavaSourceDialectWithoutSignedMementos() {
        Jcs.Obj response = RpcServer.handle(Jcs.object(
            "jsonrpc", Jcs.string("2.0"),
            "id", Jcs.integer(1),
            "method", Jcs.string("initialize")
        ));

        Jcs.Obj result = response.objectField("result");
        assertEquals("0.1.0-draft", result.stringField("version"));
        Jcs.Obj capabilities = result.objectField("capabilities");
        assertEquals("java-source", capabilities.arrayField("authoring_surfaces").stringAt(0).value());
        assertFalse(capabilities.boolField("emits_signed_mementos"));
    }

    @Test
    void liftRpcAcceptsWorkspaceAndSourcePaths() throws Exception {
        Path root = Files.createTempDirectory("provekit-java-source-lift");
        Files.writeString(root.resolve("C.java"), "class C { int f(int x) { return x + 1; } }\n");

        Jcs.Obj response = RpcServer.handle(Jcs.object(
            "jsonrpc", Jcs.string("2.0"),
            "id", Jcs.integer(2),
            "method", Jcs.string("lift"),
            "params", Jcs.object(
                "surface", Jcs.string("java-source"),
                "workspace_root", Jcs.string(root.toString()),
                "source_paths", Jcs.array(Jcs.string("C.java"))
            )
        ));

        Jcs.Obj result = response.objectField("result");
        assertEquals("ir-document", result.stringField("kind"));
        assertTrue(Jcs.encode(result).contains("C.f(int)"));
        assertTrue(result.arrayField("refusals").isEmpty());
    }
}
