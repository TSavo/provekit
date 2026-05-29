package com.provekit.emit.assertj;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.net.URI;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

import javax.tools.JavaCompiler;
import javax.tools.SimpleJavaFileObject;
import javax.tools.ToolProvider;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import com.provekit.ir.Jcs;

class AssertJEmitterTest {
    @TempDir
    Path tmp;

    @Test
    void emitsAssertJFluentAssertionsForNeutralPredicates() throws Exception {
        Object emission = emit(plan(List.of(
            predicate("concept:eq", var("a"), var("b")),
            predicate("concept:ne", var("a"), var("b")),
            predicate("concept:lt", var("a"), var("b")),
            predicate("concept:gt", var("a"), var("b")),
            predicate("concept:option-is-some", var("o")),
            predicate("concept:option-is-none", var("o"))
        )));

        String source = source(emission);

        assertTrue(source.contains("import org.junit.jupiter.api.Test;"), source);
        assertTrue(source.contains("import static org.assertj.core.api.Assertions.assertThat;"), source);
        assertTrue(source.contains("assertThat(a).isEqualTo(b);"), source);
        assertTrue(source.contains("assertThat(a).isNotEqualTo(b);"), source);
        assertTrue(source.contains("assertThat(a).isLessThan(b);"), source);
        assertTrue(source.contains("assertThat(a).isGreaterThan(b);"), source);
        assertTrue(source.contains("assertThat(o).isNotNull();"), source);
        assertTrue(source.contains("assertThat(o).isNull();"), source);
        assertEquals(List.of("eq", "ne", "lt", "gt", "option-is-some", "option-is-none"),
            emittedPredicates(emission));
        assertTrue(unsupportedPredicates(emission).isEmpty());
        assertTrue(isComplete(emission));
    }

    @Test
    void unsupportedPredicatesAreReportedAndNotEmittedAsPassingTests() throws Exception {
        Object emission = emit(plan(List.of(
            predicate("concept:eq", var("a"), var("b")),
            predicate("concept:mystery", var("a"))
        )));

        String source = source(emission);

        assertEquals(List.of("eq"), emittedPredicates(emission));
        assertEquals(List.of("mystery"), unsupportedPredicates(emission));
        assertFalse(isComplete(emission));
        assertFalse(source.contains("mystery"), source);
    }

    @Test
    void emittedAssertJSourceCompilesWithJUnitRunnerAndAssertJApi() throws Exception {
        Object emission = emit(plan(List.of(
            predicate("concept:eq", cInt(2), cInt(2)),
            predicate("concept:option-is-some", cStr("present"))
        )));

        compileGenerated(source(emission), path(emission).replace(".java", ""));
    }

    private Object emit(Object plan) throws Exception {
        Class<?> emitterClass = Class.forName("com.provekit.emit.assertj.AssertJEmitter");
        Object emitter = emitterClass.getConstructor().newInstance();
        Method emit = emitterClass.getMethod("emit", plan.getClass());
        return emit.invoke(emitter, plan);
    }

    private Object plan(List<Jcs.Obj> predicates) throws Exception {
        Class<?> planClass = Class.forName("com.provekit.emit.assertj.EmitPlan");
        Constructor<?> ctor = planClass.getConstructor(
            String.class, String.class, List.class, List.class, List.class);
        return ctor.newInstance(
            "concept:eq",
            "compare",
            List.of("a", "b", "o"),
            List.of("int", "int", "Object"),
            predicates
        );
    }

    private String source(Object emission) throws Exception {
        return (String) emission.getClass().getMethod("source").invoke(emission);
    }

    private String path(Object emission) throws Exception {
        return (String) emission.getClass().getMethod("path").invoke(emission);
    }

    @SuppressWarnings("unchecked")
    private List<String> emittedPredicates(Object emission) throws Exception {
        return (List<String>) emission.getClass().getMethod("emittedPredicates").invoke(emission);
    }

    @SuppressWarnings("unchecked")
    private List<String> unsupportedPredicates(Object emission) throws Exception {
        return (List<String>) emission.getClass().getMethod("unsupportedPredicates").invoke(emission);
    }

    private boolean isComplete(Object emission) throws Exception {
        return (boolean) emission.getClass().getMethod("isComplete").invoke(emission);
    }

    private void compileGenerated(String source, String className) throws IOException {
        JavaCompiler compiler = ToolProvider.getSystemJavaCompiler();
        assertNotNull(compiler, "system java compiler unavailable; run tests on a JDK");
        Path out = tmp.resolve("classes");
        Files.createDirectories(out);
        int rc = compiler.getTask(
            null,
            null,
            null,
            List.of("-classpath", System.getProperty("java.class.path"), "-d", out.toString()),
            null,
            List.of(new SourceFile(className, source))
        ).call() ? 0 : 1;
        assertEquals(0, rc, "emitted AssertJ source failed to compile:\n" + source);
        assertTrue(Files.exists(out.resolve(className + ".class")));
    }

    private static Jcs.Obj predicate(String name, Jcs.Json... args) {
        return Jcs.object(
            "kind", Jcs.string("op"),
            "name", Jcs.string(name),
            "args", Jcs.array(List.of(args))
        );
    }

    private static Jcs.Obj var(String name) {
        return Jcs.object("kind", Jcs.string("var"), "name", Jcs.string(name));
    }

    private static Jcs.Obj cInt(long value) {
        return Jcs.object(
            "kind", Jcs.string("const"),
            "value", Jcs.integer(value),
            "sort", Jcs.object("kind", Jcs.string("primitive"), "name", Jcs.string("Int"))
        );
    }

    private static Jcs.Obj cStr(String value) {
        return Jcs.object(
            "kind", Jcs.string("const"),
            "value", Jcs.string(value),
            "sort", Jcs.object("kind", Jcs.string("primitive"), "name", Jcs.string("String"))
        );
    }

    private static final class SourceFile extends SimpleJavaFileObject {
        private final String source;

        SourceFile(String className, String source) {
            super(URI.create("string:///" + className + ".java"), Kind.SOURCE);
            this.source = source;
        }

        @Override
        public CharSequence getCharContent(boolean ignoreEncodingErrors) {
            return source;
        }
    }
}
