package com.provekit.lift.java_source;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.provekit.ir.Jcs;
import java.util.List;
import org.junit.jupiter.api.Test;

class JavaSugarBindingLifterTest {
    @Test
    void annotatedMethodEmitsLibrarySugarBindingEntry() {
        String source = """
            package p;
            import java.net.URI;
            import java.net.http.HttpClient;
            import java.net.http.HttpRequest;
            import java.net.http.HttpResponse.BodyHandlers;
            class C {
              @ProveKitSugar(concept = "concept:http-request", library = "java-net-http")
              int fetchStatus(URI uri) throws Exception {
                return HttpClient.newHttpClient()
                    .send(HttpRequest.newBuilder(uri).build(), BodyHandlers.discarding())
                    .statusCode();
              }
            }
            """;

        JavaBindLifter.Result result = new JavaBindLifter().liftPathsFromSource("C.java", source);

        List<Jcs.Json> sugarEntries = sugarEntries(result);

        assertEquals(1, sugarEntries.size(), "expected exactly one sugar entry");

        Jcs.Obj entry = (Jcs.Obj) sugarEntries.get(0);
        assertEquals("library-sugar-binding-entry", entry.stringField("kind"));
        assertEquals("java", entry.stringField("target_language"));
        assertEquals("java-net-http", entry.stringField("target_library_tag"));
        assertEquals("concept:http-request", entry.stringField("concept_name"));
        assertEquals("fetchStatus", entry.stringField("source_function_name"));

        assertEquals(1, entry.arrayField("param_names").values().size());
        assertEquals("uri", entry.arrayField("param_names").stringAt(0).value());
        assertEquals(1, entry.arrayField("param_types").values().size());
        assertEquals("java.net.URI", entry.arrayField("param_types").stringAt(0).value());
        assertEquals("int", entry.stringField("return_type"));

        String sigCid = entry.stringField("signature_shape_cid");
        assertTrue(sigCid.startsWith("blake3-512:"), "signature_shape_cid must start with blake3-512:");
        assertEquals("blake3-512:".length() + 128, sigCid.length(), "CID must be 128 hex chars");
        assertNull(entry.get("signature_shape"), "must not emit full signature_shape document");

        assertNotNull(entry.get("term_shape"), "term_shape must be present");
        String termShapeCid = entry.stringField("term_shape_cid");
        assertTrue(termShapeCid.startsWith("blake3-512:"), "term_shape_cid must start with blake3-512:");

        Jcs.Obj bodySource = entry.objectField("body_source");
        assertEquals("C.java", bodySource.stringField("file"));
        assertNotNull(bodySource.get("span"), "body_source.span must be present");
        assertNull(bodySource.get("locator"), "must use span not locator");
        String sourceCid = bodySource.stringField("source_cid");
        assertTrue(sourceCid.startsWith("blake3-512:"), "source_cid must start with blake3-512:");

        Jcs.Obj span = bodySource.objectField("span");
        assertTrue(numberField(span, "start_line") > 0, Jcs.encode(span));
        assertTrue(numberField(span, "end_line") >= numberField(span, "start_line"), Jcs.encode(span));
        assertTrue(numberField(span, "start_col") >= 0, Jcs.encode(span));
        assertTrue(numberField(span, "end_col") >= 0, Jcs.encode(span));

        Jcs.Obj lrc = entry.objectField("loss_record_contribution");
        assertEquals("literal", lrc.stringField("form"));
        Jcs.Obj lrcValue = lrc.objectField("value");
        Jcs.Arr entries = lrcValue.arrayField("entries");
        assertTrue(entries.isEmpty(), "entries must be empty");

        assertNull(entry.get("locator"), "must not emit locator");
        assertNull(entry.get("emission_template"), "must not emit emission_template");
    }

    @Test
    void unannotatedMethodProducesZeroSugarEntries() {
        String source = """
            package p;
            class C {
              int add(int x, int y) {
                return x + y;
              }
            }
            """;

        JavaBindLifter.Result result = new JavaBindLifter().liftPathsFromSource("C.java", source);

        assertEquals(0, sugarEntries(result).size(), "unannotated method must produce zero sugar entries");
        assertTrue(bindEntries(result) > 0, "bind-lift-entry must still be emitted for unannotated method");
    }

    @Test
    void twoAnnotatedMethodsProduceTwoSugarEntries() {
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:http-request", library = "java-net-http")
              int fetchStatus(String url) {
                return 200;
              }

              @ProveKitSugar(concept = "concept:sql-query", library = "jdbc")
              String queryDb(String sql) {
                return "";
              }
            }
            """;

        JavaBindLifter.Result result = new JavaBindLifter().liftPathsFromSource("C.java", source);

        List<Jcs.Json> sugarEntries = sugarEntries(result);

        assertEquals(2, sugarEntries.size(), "two annotated methods must produce two sugar entries");

        List<String> concepts = sugarEntries.stream()
            .map(Jcs.Obj.class::cast)
            .map(e -> e.stringField("concept_name"))
            .toList();
        assertTrue(concepts.contains("concept:http-request"), concepts.toString());
        assertTrue(concepts.contains("concept:sql-query"), concepts.toString());
    }

    @Test
    void malformedSugarAnnotationProducesZeroSugarEntries() {
        String sourceMissingLib = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:http-request")
              int missingLib(String url) {
                return 0;
              }
            }
            """;

        JavaBindLifter.Result r1 = new JavaBindLifter().liftPathsFromSource("C.java", sourceMissingLib);
        assertEquals(0, sugarEntries(r1).size(), "missing library must produce zero sugar entries");

        String sourceMissingConcept = """
            package p;
            class C {
              @ProveKitSugar(library = "java-net-http")
              int missingConcept(String url) {
                return 0;
              }
            }
            """;

        JavaBindLifter.Result r2 = new JavaBindLifter().liftPathsFromSource("C.java", sourceMissingConcept);
        assertEquals(0, sugarEntries(r2).size(), "missing concept must produce zero sugar entries");
    }

    private static List<Jcs.Json> sugarEntries(JavaBindLifter.Result result) {
        return result.entries().stream()
            .filter(e -> {
                if (!(e instanceof Jcs.Obj obj)) return false;
                return "library-sugar-binding-entry".equals(obj.stringFieldOrNull("kind"));
            })
            .toList();
    }

    private static long bindEntries(JavaBindLifter.Result result) {
        return result.entries().stream()
            .filter(e -> {
                if (!(e instanceof Jcs.Obj obj)) return false;
                return "bind-lift-entry".equals(obj.stringFieldOrNull("kind"));
            })
            .count();
    }

    private static long numberField(Jcs.Obj obj, String key) {
        Jcs.Json value = obj.get(key);
        if (value instanceof Jcs.Num n) return n.value();
        throw new IllegalArgumentException("field is not a number: " + key);
    }
}
