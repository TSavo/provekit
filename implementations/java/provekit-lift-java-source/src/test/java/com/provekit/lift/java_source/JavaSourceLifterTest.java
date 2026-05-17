package com.provekit.lift.java_source;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.provekit.ir.Jcs;
import java.util.List;
import org.junit.jupiter.api.Test;

class JavaSourceLifterTest {
    @Test
    void liftsSingleReturnMethodAndWrapsFileInJavaSourceUnit() {
        String source = """
            package p;
            class C {
              int add(int x, int y) {
                return x + y;
              }
            }
            """;

        JavaSourceLifter.LiftResult result = new JavaSourceLifter().liftSource("C.java", source);

        assertTrue(result.refusals().isEmpty(), result.refusals().toString());
        assertEquals(2, result.declarations().size());

        Jcs.Obj sourceUnitContract = (Jcs.Obj) result.declarations().get(0);
        assertEquals("<source-unit:C.java>", sourceUnitContract.stringField("fnName"));
        Jcs.Obj sourceUnitTerm = result.sourceUnitTerm();
        assertEquals("java:source-unit", sourceUnitTerm.stringField("name"));
        assertEquals(source, sourceUnitTerm.arrayField("args").stringAt(0).value());

        Jcs.Obj methodContract = (Jcs.Obj) result.declarations().get(1);
        assertEquals("p.C.add(int,int)", methodContract.stringField("fnName"));
        Jcs.Obj post = methodContract.objectField("post");
        Jcs.Obj liftedValue = (Jcs.Obj) post.arrayField("args").get(1);
        assertEquals("java:add", liftedValue.stringField("name"));
        assertAllOpsAreJavaNamespaced(sourceUnitTerm);
    }

    @Test
    void overloadedMethodsUseErasedParameterTypesInFunctionNames() {
        String source = """
            class Over {
              int f(int x) { return x; }
              int f(long x) { return (int) x; }
            }
            """;

        JavaSourceLifter.LiftResult result = new JavaSourceLifter().liftSource("Over.java", source);

        assertTrue(result.refusals().isEmpty(), result.refusals().toString());
        List<String> fnNames = result.declarations().stream()
            .map(Jcs.Obj.class::cast)
            .map(o -> o.stringField("fnName"))
            .toList();
        assertTrue(fnNames.contains("Over.f(int)"), fnNames.toString());
        assertTrue(fnNames.contains("Over.f(long)"), fnNames.toString());
    }

    @Test
    void unsupportedSyntaxIsRefusedInsteadOfLoweredToUnknownOps() {
        String source = """
            import java.util.function.IntUnaryOperator;
            class C {
              int f(int x) {
                IntUnaryOperator op = y -> y + 1;
                return op.applyAsInt(x);
              }
            }
            """;

        JavaSourceLifter.LiftResult result = new JavaSourceLifter().liftSource("C.java", source);

        assertFalse(result.refusals().isEmpty());
        JavaSourceLifter.Refusal refusal = result.refusals().get(0);
        assertEquals("C.f(int)", refusal.function());
        assertTrue(refusal.reason().contains("LAMBDA"), refusal.toString());
        assertEquals(
            JavaSourceLifter.examQuestionCidFor("sort-classification", "concept:Term", "java").orElseThrow(),
            refusal.examQuestionCid());
        assertEquals(JavaSourceLifter.EXAM_MANIFEST_CID, refusal.examManifestCid());
        assertEquals(0, result.declarations().stream()
            .map(Jcs.Obj.class::cast)
            .filter(o -> "C.f(int)".equals(o.stringField("fnName")))
            .count());
        assertFalse(Jcs.encode(result.toJson()).contains("java:unknown"));
    }

    @Test
    void unsupportedLambdaRefusalDoesNotFireReturnSortVariant() {
        String source = """
            import java.util.function.IntUnaryOperator;
            class C {
              int f(int x) {
                IntUnaryOperator op = y -> y + 1;
                return op.applyAsInt(x);
              }
            }
            """;

        JavaSourceLifter.LiftResult result = new JavaSourceLifter().liftSource("C.java", source);

        assertEquals(1, result.refusals().size());
        assertEquals("C.f(int)", result.refusals().get(0).function());
        assertFalse(result.refusals().stream().anyMatch(r -> "unsupported-return-sort".equals(r.kind())));
    }

    @Test
    void unsupportedLambdaRefusalCitesTermQuestionNotRelatedSort() {
        String source = """
            import java.util.function.IntUnaryOperator;
            class C {
              int f(int x) {
                IntUnaryOperator op = y -> y + 1;
                return op.applyAsInt(x);
              }
            }
            """;

        JavaSourceLifter.LiftResult result = new JavaSourceLifter().liftSource("C.java", source);

        String refusalCid = result.refusals().get(0).examQuestionCid();
        String related = JavaSourceLifter.examQuestionCidFor(
            "sort-classification",
            "concept:Formula",
            "java").orElseThrow();
        assertFalse(refusalCid.equals(related));
    }

    @Test
    void effectsUseCanonicalWireShapesAndSortOrder() {
        String source = """
            class C {
              int field;
              int f(int x) {
                while (x > 0) {
                  field = g(field);
                  x = x - 1;
                }
                return field;
              }
              int g(int z) { return z; }
            }
            """;

        JavaSourceLifter.LiftResult result = new JavaSourceLifter().liftSource("C.java", source);

        assertTrue(result.refusals().isEmpty(), result.refusals().toString());
        Jcs.Obj contract = result.declarations().stream()
            .map(Jcs.Obj.class::cast)
            .filter(o -> "C.f(int)".equals(o.stringField("fnName")))
            .findFirst()
            .orElseThrow();
        Jcs.Arr effects = contract.arrayField("effects");

        assertEquals("reads", effects.objectAt(0).stringField("kind"));
        assertEquals("writes", effects.objectAt(1).stringField("kind"));
        assertEquals("unresolved_call", effects.objectAt(2).stringField("kind"));
        assertEquals("name", effects.objectAt(2).fields().get(1).key());
        assertEquals("opaque_loop", effects.objectAt(3).stringField("kind"));
        assertTrue(effects.objectAt(3).stringField("loopCid").startsWith("blake3-512:"));
    }

    @Test
    void sourceUnitCompilerRoundTripsToByteIdenticalLiftedTerm() {
        String source = """
            class C {
              int add(int x, int y) {
                return x + y;
              }
            }
            """;

        JavaSourceLifter lifter = new JavaSourceLifter();
        JavaSourceLifter.LiftResult first = lifter.liftSource("C.java", source);
        String compiled = new JavaSourceCompiler().compile(first.sourceUnitTerm());
        JavaSourceLifter.LiftResult second = lifter.liftSource("C.java", compiled);

        assertEquals(Jcs.encode(first.sourceUnitTerm()), Jcs.encode(second.sourceUnitTerm()));
    }

    private static void assertAllOpsAreJavaNamespaced(Jcs.Json value) {
        if (value instanceof Jcs.Obj obj) {
            String kind = obj.stringFieldOrNull("kind");
            String name = obj.stringFieldOrNull("name");
            if (("ctor".equals(kind) || "op".equals(kind)) && name != null) {
                assertTrue(name.startsWith("java:"), name);
            }
            for (Jcs.Field field : obj.fields()) {
                assertAllOpsAreJavaNamespaced(field.value());
            }
        } else if (value instanceof Jcs.Arr arr) {
            for (Jcs.Json item : arr.values()) {
                assertAllOpsAreJavaNamespaced(item);
            }
        }
    }
}
