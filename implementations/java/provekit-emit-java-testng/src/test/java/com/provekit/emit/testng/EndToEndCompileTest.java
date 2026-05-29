package com.provekit.emit.testng;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

import javax.tools.JavaCompiler;
import javax.tools.ToolProvider;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import com.provekit.ir.Jcs;

/** End-to-end: atomic ProofIR-shaped predicates emit native compiling TestNG source. */
class EndToEndCompileTest {

    private final TestNgEmitter emitter = new TestNgEmitter();

    @Test
    void atomicEqEmitsCompilingTestNgTest(@TempDir Path dir) throws Exception {
        EmitPlan plan = new EmitPlan(
            "concept:eq",
            "identity",
            List.of("a", "b"),
            List.of("int", "int"),
            List.of((Jcs.Obj) Jcs.parse(
                "{\"kind\":\"atomic\",\"name\":\"eq\",\"args\":["
                + "{\"kind\":\"const\",\"value\":2,\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}},"
                + "{\"kind\":\"const\",\"value\":2,\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}]}")));

        TestNgEmitter.Emission e = emitter.emit(plan);
        assertTrue(e.source().contains("Assert.assertEquals(2, 2);"), e.source());
        assertCompiles(dir, "IdentityContractTest", e.source());
    }

    @Test
    void everySupportedAtomicPredicateWithVarOperandsCompiles(@TempDir Path dir) throws Exception {
        EmitPlan plan = new EmitPlan(
            "concept:mixed",
            "mixed",
            List.of("a", "b", "o"),
            List.of("int", "int", "Object"),
            List.of(
                binaryVar("eq", "a", "b"),
                binaryVar("ne", "a", "b"),
                binaryVar("lt", "a", "b"),
                binaryVar("gt", "a", "b"),
                binaryVar("le", "a", "b"),
                binaryVar("ge", "a", "b"),
                unaryVar("option-is-some", "o"),
                unaryVar("option-is-none", "o"),
                unaryVar("fallible-err", "o")));

        TestNgEmitter.Emission e = emitter.emit(plan);
        assertEquals(
            List.of("eq", "ne", "lt", "gt", "le", "ge",
                    "option-is-some", "option-is-none", "fallible-err"),
            e.emittedPredicates());
        assertTrue(e.source().contains("int a = 0;"), e.source());
        assertTrue(e.source().contains("Object o = null;"), e.source());
        assertCompiles(dir, "MixedContractTest", e.source());
    }

    private static Jcs.Obj binaryVar(String concept, String a, String b) {
        return (Jcs.Obj) Jcs.parse(
            "{\"kind\":\"atomic\",\"name\":\"" + concept + "\",\"args\":["
            + "{\"kind\":\"var\",\"name\":\"" + a + "\"},"
            + "{\"kind\":\"var\",\"name\":\"" + b + "\"}]}");
    }

    private static Jcs.Obj unaryVar(String concept, String x) {
        return (Jcs.Obj) Jcs.parse(
            "{\"kind\":\"atomic\",\"name\":\"" + concept + "\",\"args\":["
            + "{\"kind\":\"var\",\"name\":\"" + x + "\"}]}");
    }

    private static void assertCompiles(Path dir, String className, String source)
            throws Exception {
        JavaCompiler compiler = ToolProvider.getSystemJavaCompiler();
        assertNotNull(compiler, "system java compiler unavailable (run on a JDK, not a JRE)");

        Path sourceFile = dir.resolve(className + ".java");
        Files.writeString(sourceFile, source, StandardCharsets.UTF_8);

        Path outDir = dir.resolve("out");
        Files.createDirectories(outDir);

        ByteArrayOutputStream err = new ByteArrayOutputStream();
        String classpath = System.getProperty("java.class.path", "");
        int rc = compiler.run(
            null, null, err,
            "-classpath", classpath,
            "-d", outDir.toString(),
            sourceFile.toString());

        assertEquals(0, rc,
            "emitted TestNG source failed to compile:\n" + source
            + "\n--- javac diagnostics ---\n"
            + err.toString(StandardCharsets.UTF_8));
        assertTrue(Files.exists(outDir.resolve(className + ".class")),
            "no .class produced for " + className);
    }
}
