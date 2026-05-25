package com.provekit.emit.junit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.provekit.ir.Jcs;

/** Class-level emission behavior: imports, class shell, gap accounting, CID. */
class JUnitEmitterTest {

    private final JUnitEmitter emitter = new JUnitEmitter();

    private static List<Jcs.Obj> predicates(String... json) {
        java.util.List<Jcs.Obj> out = new java.util.ArrayList<>();
        for (String j : json) out.add((Jcs.Obj) Jcs.parse(j));
        return out;
    }

    @Test
    void emitsTestClassWithImportsAndMethodPerPredicate() {
        EmitPlan plan = new EmitPlan(
            "concept:ge",
            "clamp",
            List.of("x", "lo"),
            List.of("int", "int"),
            predicates(
                "{\"kind\":\"op\",\"name\":\"concept:ge\",\"args\":["
                    + "{\"kind\":\"var\",\"name\":\"x\"},"
                    + "{\"kind\":\"var\",\"name\":\"lo\"}]}",
                "{\"kind\":\"op\",\"name\":\"concept:le\",\"args\":["
                    + "{\"kind\":\"var\",\"name\":\"x\"},"
                    + "{\"kind\":\"var\",\"name\":\"hi\"}]}"));

        JUnitEmitter.Emission e = emitter.emit(plan);
        String src = e.source();

        assertTrue(src.contains("import org.junit.jupiter.api.Test;"), src);
        assertTrue(src.contains("import static org.junit.jupiter.api.Assertions.*;"), src);
        assertTrue(src.contains("public class ClampContractTest {"), src);
        assertTrue(src.contains("@Test"), src);
        assertTrue(src.contains("assertTrue(x >= lo);"), src);
        assertTrue(src.contains("assertTrue(x <= hi);"), src);
        assertEquals("ClampContractTest.java", e.path());
        assertEquals(List.of("ge", "le"), e.emittedPredicates());
        assertTrue(e.unsupportedPredicates().isEmpty());
        assertTrue(e.isComplete());
        assertTrue(e.artifactCid().startsWith("blake3-512:"), e.artifactCid());
    }

    @Test
    void unsupportedPredicateRecordedAsGapNotEmitted() {
        EmitPlan plan = new EmitPlan(
            "concept:eq",
            "f",
            List.of("a"),
            List.of("int"),
            predicates(
                "{\"kind\":\"op\",\"name\":\"concept:eq\",\"args\":["
                    + "{\"kind\":\"var\",\"name\":\"a\"},"
                    + "{\"kind\":\"var\",\"name\":\"b\"}]}",
                "{\"kind\":\"op\",\"name\":\"concept:mystery\",\"args\":["
                    + "{\"kind\":\"var\",\"name\":\"a\"}]}"));

        JUnitEmitter.Emission e = emitter.emit(plan);
        assertEquals(List.of("eq"), e.emittedPredicates());
        assertEquals(List.of("mystery"), e.unsupportedPredicates());
        assertFalse(e.isComplete(), "incomplete when any predicate is unsupported");
        assertFalse(e.source().contains("mystery"), e.source());
    }

    @Test
    void cidIsDeterministicForSameSource() {
        EmitPlan plan = new EmitPlan(
            "concept:option-is-some", "lookup", List.of("o"), List.of("Object"),
            predicates("{\"kind\":\"op\",\"name\":\"concept:option-is-some\",\"args\":["
                + "{\"kind\":\"var\",\"name\":\"o\"}]}"));
        assertEquals(emitter.emit(plan).artifactCid(), emitter.emit(plan).artifactCid());
    }

    @Test
    void jsonResponseRoundTripsThroughParser() {
        EmitPlan plan = new EmitPlan(
            "concept:eq", "f", List.of("a"), List.of("int"),
            predicates("{\"kind\":\"op\",\"name\":\"concept:eq\",\"args\":["
                + "{\"kind\":\"var\",\"name\":\"a\"},"
                + "{\"kind\":\"var\",\"name\":\"b\"}]}"));
        String json = emitter.emit(plan).toJson();
        Jcs.Json parsed = Jcs.parse(json);
        assertTrue(parsed instanceof Jcs.Obj, json);
        assertEquals("FContractTest.java", ((Jcs.Obj) parsed).stringFieldOrNull("path"));
        assertEquals("java", ((Jcs.Obj) parsed).stringFieldOrNull("extension"));
    }
}
