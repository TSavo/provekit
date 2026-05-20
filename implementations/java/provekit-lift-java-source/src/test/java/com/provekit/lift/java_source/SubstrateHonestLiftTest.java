// SPDX-License-Identifier: Apache-2.0
//
// JUnit 5 tests for substrate-honest lifter extensions (paper 24 parity with walk_rpc.rs):
//   - @ProveKitSugar.loss() -> loss_record_contribution.value.entries
//   - @ProveKitSugar.observedDimension() -> observed_dimension on entry
//   - @ProveKitRefuse -> refusal-memento IR record
//
// Discrimination tests per variant (three per enum case):
//   positive + discrimination (wrong value does not match) + structural (field presence/shape).

package com.provekit.lift.java_source;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.assertFalse;

import com.provekit.ir.Jcs;
import java.util.List;
import org.junit.jupiter.api.Test;

class SubstrateHonestLiftTest {

    // =========================================================================
    // A. loss[] = {} (exact materialization -- entries must be empty array)
    // =========================================================================

    @Test
    void sugarWithEmptyLossEmitsEmptyLossEntries() {
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:sql-execute", library = "sqlite-jdbc",
                             loss = {})
              int exec(String sql) { return 0; }
            }
            """;
        Jcs.Obj entry = firstSugarEntry(source);
        Jcs.Obj lrc = entry.objectField("loss_record_contribution");
        assertEquals("literal", lrc.stringField("form"));
        Jcs.Arr entries = lrc.objectField("value").arrayField("entries");
        assertTrue(entries.isEmpty(), "empty loss must produce empty entries array");
    }

    @Test
    void sugarWithEmptyLossDiscrimination_notNullEntries() {
        // Discrimination: the entries field must be present even when empty.
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:sql-execute", library = "sqlite-jdbc",
                             loss = {})
              int exec(String sql) { return 0; }
            }
            """;
        Jcs.Obj entry = firstSugarEntry(source);
        Jcs.Obj lrc = entry.objectField("loss_record_contribution");
        assertNotNull(lrc.get("value"), "loss_record_contribution.value must be present");
        assertNotNull(lrc.objectField("value").get("entries"), "entries must be present");
    }

    @Test
    void sugarWithEmptyLossStructural_formIsLiteral() {
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:sql-execute", library = "sqlite-jdbc",
                             loss = {})
              int exec(String sql) { return 0; }
            }
            """;
        Jcs.Obj entry = firstSugarEntry(source);
        assertEquals("literal", entry.objectField("loss_record_contribution").stringField("form"),
            "form must be 'literal'");
        assertNull(entry.get("observed_dimension"), "no observedDimension means no observed_dimension field");
    }

    // =========================================================================
    // B. loss[] with multiple dimensions
    // =========================================================================

    @Test
    void sugarWithMultiDimLossEmitsAllEntries() {
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:sql-connection-open", library = "sqlite-jdbc",
                             loss = {"sync-vs-async", "auth-mechanism", "connection-pooling"})
              Object open(String url) throws Exception { return null; }
            }
            """;
        Jcs.Obj entry = firstSugarEntry(source);
        Jcs.Arr entries = entry.objectField("loss_record_contribution").objectField("value").arrayField("entries");
        assertEquals(3, entries.values().size(), "must emit 3 loss entries");
        List<String> dims = entries.values().stream()
            .map(v -> ((Jcs.Str) v).value())
            .toList();
        assertTrue(dims.contains("sync-vs-async"), dims.toString());
        assertTrue(dims.contains("auth-mechanism"), dims.toString());
        assertTrue(dims.contains("connection-pooling"), dims.toString());
    }

    @Test
    void sugarWithMultiDimLossDiscrimination_oneDimDiffers() {
        // Discrimination: single-dim loss must not equal multi-dim result.
        String sourceSingle = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:sql-connection-open", library = "sqlite-jdbc",
                             loss = {"sync-vs-async"})
              Object open(String url) throws Exception { return null; }
            }
            """;
        Jcs.Arr entries = firstSugarEntry(sourceSingle)
            .objectField("loss_record_contribution").objectField("value").arrayField("entries");
        assertEquals(1, entries.values().size(), "single-dim loss must emit 1 entry");
    }

    @Test
    void sugarWithMultiDimLossStructural_noObservedDimension() {
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:sql-connection-open", library = "sqlite-jdbc",
                             loss = {"sync-vs-async", "auth-mechanism"})
              Object open(String url) throws Exception { return null; }
            }
            """;
        Jcs.Obj entry = firstSugarEntry(source);
        assertNull(entry.get("observed_dimension"),
            "multi-dim sugar without observedDimension must not emit observed_dimension");
        assertEquals("library-sugar-binding-entry", entry.stringField("kind"));
    }

    // =========================================================================
    // C. observedDimension
    // =========================================================================

    @Test
    void sugarWithObservedDimensionEmitsField() {
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:contract-observation", library = "sqlite-jdbc",
                             observedDimension = "autocommit-mode")
              boolean isAutoCommit(Object conn) throws Exception { return true; }
            }
            """;
        Jcs.Obj entry = firstSugarEntry(source);
        assertEquals("autocommit-mode", entry.stringFieldOrNull("observed_dimension"),
            "observed_dimension must be propagated to the entry");
    }

    @Test
    void sugarWithObservedDimensionDiscrimination_differentDimension() {
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:contract-observation", library = "sqlite-jdbc",
                             observedDimension = "write-permission")
              boolean isReadOnly(Object conn) throws Exception { return false; }
            }
            """;
        Jcs.Obj entry = firstSugarEntry(source);
        assertEquals("write-permission", entry.stringFieldOrNull("observed_dimension"));
        // Must not be autocommit-mode (from prior test).
        assertFalse("autocommit-mode".equals(entry.stringFieldOrNull("observed_dimension")));
    }

    @Test
    void sugarWithObservedDimensionStructural_conceptIsObservation() {
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:contract-observation", library = "sqlite-jdbc",
                             observedDimension = "column-count")
              int getColumnCount(Object rs) throws Exception { return 0; }
            }
            """;
        Jcs.Obj entry = firstSugarEntry(source);
        assertEquals("concept:contract-observation", entry.stringField("concept_name"));
        assertNotNull(entry.get("observed_dimension"));
        assertEquals("column-count", entry.stringField("observed_dimension"));
        // observed_dimension must not appear in loss_record_contribution entries.
        Jcs.Arr lossEntries = entry.objectField("loss_record_contribution").objectField("value").arrayField("entries");
        assertTrue(lossEntries.isEmpty(), "no loss for pure observation binding");
    }

    // =========================================================================
    // D. @ProveKitRefuse
    // =========================================================================

    @Test
    void refuseAnnotationEmitsRefusalMemento() {
        String source = """
            package p;
            class Outer {
              @ProveKitRefuse(
                surface = "java.sql.Connection#backup",
                concept = "concept:sql-physical-backup",
                reason = "No JDBC-standard physical backup method.",
                wouldCloseWithCluster = "Physical backup on >=2 SQL drivers"
              )
              static final class RefusedBackup {}
            }
            """;
        JavaBindLifter.Result result = new JavaBindLifter().liftPathsFromSource("C.java", source);
        List<Jcs.Json> refusals = refusalEntries(result);
        assertEquals(1, refusals.size(), "expected exactly one refusal-memento entry");

        Jcs.Obj r = (Jcs.Obj) refusals.get(0);
        assertEquals("refusal-memento", r.stringField("kind"));
        assertEquals("concept:sql-physical-backup", r.stringField("concept"));
        assertEquals("java.sql.Connection#backup", r.stringField("surface"));
        assertEquals("java", r.stringField("target_language"));
    }

    @Test
    void refuseAnnotationDiscrimination_conceptAndSurfaceArePaired() {
        // Discrimination: two distinct @ProveKitRefuse produce two distinct refusal entries.
        String source = """
            package p;
            class Outer {
              @ProveKitRefuse(
                surface = "java.sql.Connection#backup",
                concept = "concept:sql-physical-backup",
                reason = "No JDBC backup.",
                wouldCloseWithCluster = "Physical backup on >=2 SQL drivers"
              )
              static final class RefusedBackup {}

              @ProveKitRefuse(
                surface = "java.sql.Connection#createBlob",
                concept = "concept:sql-blob-handle",
                reason = "Incremental BLOB I/O differs per driver.",
                wouldCloseWithCluster = "Incremental BLOB I/O on >=2 SQL drivers"
              )
              static final class RefusedBlob {}
            }
            """;
        List<Jcs.Json> refusals = refusalEntries(new JavaBindLifter().liftPathsFromSource("C.java", source));
        assertEquals(2, refusals.size(), "two @ProveKitRefuse must emit two refusal-memento entries");
        List<String> concepts = refusals.stream()
            .map(Jcs.Obj.class::cast)
            .map(r -> r.stringField("concept"))
            .toList();
        assertTrue(concepts.contains("concept:sql-physical-backup"), concepts.toString());
        assertTrue(concepts.contains("concept:sql-blob-handle"), concepts.toString());
    }

    @Test
    void refuseAnnotationStructural_allFourFieldsPresent() {
        String source = """
            package p;
            class Outer {
              @ProveKitRefuse(
                surface = "java.sql.Connection#backup",
                concept = "concept:sql-physical-backup",
                reason = "No JDBC backup method.",
                wouldCloseWithCluster = "Physical backup on >=2 SQL drivers"
              )
              static final class RefusedBackup {}
            }
            """;
        Jcs.Obj r = (Jcs.Obj) refusalEntries(
            new JavaBindLifter().liftPathsFromSource("C.java", source)).get(0);
        assertNotNull(r.get("surface"), "surface must be present");
        assertNotNull(r.get("concept"), "concept must be present");
        assertNotNull(r.get("reason"), "reason must be present");
        assertNotNull(r.get("would_close_with_cluster"), "would_close_with_cluster must be present");
        assertNotNull(r.get("target_language"), "target_language must be present");
        assertEquals("java", r.stringField("target_language"));
        assertEquals("No JDBC backup method.", r.stringField("reason"));
        assertEquals("Physical backup on >=2 SQL drivers", r.stringField("would_close_with_cluster"));
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    private static Jcs.Obj firstSugarEntry(String source) {
        JavaBindLifter.Result result = new JavaBindLifter().liftPathsFromSource("C.java", source);
        List<Jcs.Json> entries = result.entries().stream()
            .filter(e -> e instanceof Jcs.Obj obj
                && "library-sugar-binding-entry".equals(obj.stringFieldOrNull("kind")))
            .toList();
        assertFalse(entries.isEmpty(), "expected at least one sugar entry; got: " + result.entries());
        return (Jcs.Obj) entries.get(0);
    }

    private static List<Jcs.Json> refusalEntries(JavaBindLifter.Result result) {
        return result.entries().stream()
            .filter(e -> e instanceof Jcs.Obj obj
                && "refusal-memento".equals(obj.stringFieldOrNull("kind")))
            .toList();
    }
}
