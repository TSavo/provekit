package com.provekit.emit.junit;

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

/**
 * End-to-end: a real catalog concept goes IN as a contract, a JUnit5 test
 * method comes OUT, and the emitted source COMPILES under JUnit5.
 *
 * <p>This is the most load-bearing check in the kit: it proves the inline
 * mapping produces genuinely compilable java with correct imports and the
 * right {@code Assertions} surface, not just strings that look right.
 */
class EndToEndCompileTest {

    private final JUnitEmitter emitter = new JUnitEmitter();

    @Test
    void conceptEqEmitsCompilingJUnitTest(@TempDir Path dir) throws Exception {
        // concept:eq(actual, expected) — a real abstraction in the catalog.
        EmitPlan plan = new EmitPlan(
            "concept:eq",
            "identity",
            List.of("a", "b"),
            List.of("int", "int"),
            List.of((Jcs.Obj) Jcs.parse(
                "{\"kind\":\"op\",\"name\":\"concept:eq\",\"args\":["
                + "{\"kind\":\"const\",\"value\":2,\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}},"
                + "{\"kind\":\"const\",\"value\":2,\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}]}")));

        JUnitEmitter.Emission e = emitter.emit(plan);
        assertTrue(e.source().contains("assertEquals(2, 2);"), e.source());
        assertCompiles(dir, "IdentityContractTest", e.source());
    }

    @Test
    void conceptOptionIsSomeEmitsCompilingJUnitTest(@TempDir Path dir) throws Exception {
        // concept:option-is-some(o) -> assertNotNull(o); realized in the
        // catalog as java:objects-nonnull.
        EmitPlan plan = new EmitPlan(
            "concept:option-is-some",
            "lookup",
            List.of(),
            List.of(),
            List.of((Jcs.Obj) Jcs.parse(
                "{\"kind\":\"op\",\"name\":\"concept:option-is-some\",\"args\":["
                + "{\"kind\":\"const\",\"value\":\"present\","
                + "\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}]}")));

        JUnitEmitter.Emission e = emitter.emit(plan);
        assertTrue(e.source().contains("assertNotNull(\"present\");"), e.source());
        assertCompiles(dir, "LookupContractTest", e.source());
    }

    @Test
    void multiPredicateContractCompiles(@TempDir Path dir) throws Exception {
        // A contract with several supported predicates, all using literals so
        // the emitted assertions are self-contained and compilable.
        EmitPlan plan = new EmitPlan(
            "concept:range",
            "bounded",
            List.of(),
            List.of(),
            List.of(
                (Jcs.Obj) Jcs.parse(
                    "{\"kind\":\"op\",\"name\":\"concept:gt\",\"args\":["
                    + lit(5) + "," + lit(0) + "]}"),
                (Jcs.Obj) Jcs.parse(
                    "{\"kind\":\"op\",\"name\":\"concept:lt\",\"args\":["
                    + lit(5) + "," + lit(10) + "]}"),
                (Jcs.Obj) Jcs.parse(
                    "{\"kind\":\"op\",\"name\":\"concept:ne\",\"args\":["
                    + lit(5) + "," + lit(0) + "]}")));

        JUnitEmitter.Emission e = emitter.emit(plan);
        assertEquals(List.of("gt", "lt", "ne"), e.emittedPredicates());
        assertCompiles(dir, "BoundedContractTest", e.source());
    }

    private static String lit(long v) {
        return "{\"kind\":\"const\",\"value\":" + v
            + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";
    }

    /**
     * Write {@code source} to {@code <className>.java} in {@code dir} and
     * invoke the system javac with the current test classpath (which carries
     * junit-jupiter, since this very test runs under JUnit5). Fails the test
     * if compilation does not succeed.
     */
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
            "emitted JUnit5 source failed to compile:\n" + source
            + "\n--- javac diagnostics ---\n"
            + err.toString(StandardCharsets.UTF_8));
        assertTrue(Files.exists(outDir.resolve(className + ".class")),
            "no .class produced for " + className);
    }
}
