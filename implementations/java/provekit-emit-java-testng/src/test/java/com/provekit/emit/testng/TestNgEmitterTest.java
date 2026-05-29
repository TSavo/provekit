package com.provekit.emit.testng;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;

import org.junit.jupiter.api.Test;

import com.provekit.ir.Jcs;

/** Class-level TestNG emission behavior: imports, assertion shell, gaps, CID. */
class TestNgEmitterTest {

    private final TestNgEmitter emitter = new TestNgEmitter();

    private static List<Jcs.Obj> predicates(String... json) {
        java.util.List<Jcs.Obj> out = new java.util.ArrayList<>();
        for (String j : json) out.add((Jcs.Obj) Jcs.parse(j));
        return out;
    }

    @Test
    void emitsNativeTestNgClassFromAtomicPredicates() {
        EmitPlan plan = new EmitPlan(
            "concept:range",
            "clamp",
            List.of("x", "lo", "hi"),
            List.of("int", "int", "int"),
            predicates(
                "{\"kind\":\"atomic\",\"name\":\"ge\",\"args\":["
                    + "{\"kind\":\"var\",\"name\":\"x\"},"
                    + "{\"kind\":\"var\",\"name\":\"lo\"}]}",
                "{\"kind\":\"atomic\",\"name\":\"le\",\"args\":["
                    + "{\"kind\":\"var\",\"name\":\"x\"},"
                    + "{\"kind\":\"var\",\"name\":\"hi\"}]}"));

        TestNgEmitter.Emission e = emitter.emit(plan);
        String src = e.source();

        assertTrue(src.contains("import org.testng.annotations.Test;"), src);
        assertTrue(src.contains("import org.testng.Assert;"), src);
        assertTrue(src.contains("public class ClampContractTest {"), src);
        assertTrue(src.contains("@Test"), src);
        assertTrue(src.contains("public void verifiesGe_0()"), src);
        assertTrue(src.contains("Assert.assertTrue(x >= lo);"), src);
        assertTrue(src.contains("Assert.assertTrue(x <= hi);"), src);
        assertEquals("ClampContractTest.java", e.path());
        assertEquals(List.of("ge", "le"), e.emittedPredicates());
        assertTrue(e.unsupportedPredicates().isEmpty());
        assertTrue(e.isComplete());
        assertTrue(e.artifactCid().startsWith("blake3-512:"), e.artifactCid());
    }

    @Test
    void unsupportedPredicateRecordedAsGapNotFakeTest() {
        EmitPlan plan = new EmitPlan(
            "concept:eq",
            "f",
            List.of("a"),
            List.of("int"),
            predicates(
                "{\"kind\":\"atomic\",\"name\":\"eq\",\"args\":["
                    + "{\"kind\":\"var\",\"name\":\"a\"},"
                    + "{\"kind\":\"var\",\"name\":\"b\"}]}",
                "{\"kind\":\"atomic\",\"name\":\"mystery\",\"args\":["
                    + "{\"kind\":\"var\",\"name\":\"a\"}]}"));

        TestNgEmitter.Emission e = emitter.emit(plan);
        assertEquals(List.of("eq"), e.emittedPredicates());
        assertEquals(List.of("mystery"), e.unsupportedPredicates());
        assertFalse(e.isComplete(), "incomplete when any predicate is unsupported");
        assertFalse(e.source().contains("mystery"), e.source());
        assertTrue(e.toJson().contains("\"unsupported_predicates\":[\"mystery\"]"));
        assertTrue(e.toJson().contains("\"is_complete\":false"));
    }

    @Test
    void jsonResponseRoundTripsThroughParser() {
        EmitPlan plan = new EmitPlan(
            "concept:eq", "f", List.of("a"), List.of("int"),
            predicates("{\"kind\":\"atomic\",\"name\":\"eq\",\"args\":["
                + "{\"kind\":\"var\",\"name\":\"a\"},"
                + "{\"kind\":\"var\",\"name\":\"b\"}]}"));
        String json = emitter.emit(plan).toJson();
        Jcs.Json parsed = Jcs.parse(json);
        assertTrue(parsed instanceof Jcs.Obj, json);
        assertEquals("testng-test-emission", ((Jcs.Obj) parsed).stringFieldOrNull("kind"));
        assertEquals("FContractTest.java", ((Jcs.Obj) parsed).stringFieldOrNull("path"));
        assertEquals("java", ((Jcs.Obj) parsed).stringFieldOrNull("extension"));
    }
}
