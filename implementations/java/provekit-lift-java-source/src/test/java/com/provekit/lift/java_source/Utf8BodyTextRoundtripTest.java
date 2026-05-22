// SPDX-License-Identifier: Apache-2.0
//
// Regression test for issue #1372: JavaBindLifter body-text extraction must
// preserve UTF-8 characters verbatim. Previously Files.readString(Path) used
// the platform default charset (Latin-1 on some JVMs), causing U+00A7 § to
// emit as Â§ (the Latin-1 misread of the two-byte UTF-8 sequence C2 A7).
//
// Three test variants per discrimination rule:
//   positive    — § in body comment round-trips through liftPathsFromSource
//   discrimination — plain ASCII body does not produce § in body_text
//   structural  — body_source.body_text field is always present on sugar entries

package com.provekit.lift.java_source;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.provekit.ir.Jcs;
import java.util.List;
import org.junit.jupiter.api.Test;

class Utf8BodyTextRoundtripTest {

    // =========================================================================
    // Positive: non-ASCII characters in a method body survive the lifter
    // =========================================================================

    @Test
    void sectionSignRoundtripsInBodyText() {
        // § U+00A7, checkmark U+2713, and target emoji U+1F3AF all in a comment
        // inside the method body. The lifter must emit them byte-identical.
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:sql-query", library = "jdbc")
              String query(String sql) {
                // RFC 8785 §7.1.12.1 -> checkmark ✓ -> target 🎯
                return sql;
              }
            }
            """;

        JavaBindLifter.Result result = new JavaBindLifter().liftPathsFromSource("C.java", source);

        List<Jcs.Json> entries = sugarEntries(result);
        assertEquals(1, entries.size(), "expected one sugar entry");

        Jcs.Obj entry = (Jcs.Obj) entries.get(0);
        Jcs.Obj bodySource = entry.objectField("body_source");
        String bodyText = bodySource.stringField("body_text");

        assertTrue(bodyText.contains("§"),
            "body_text must contain U+00A7 (§) verbatim; got: " + bodyText);
        assertTrue(bodyText.contains("✓"),
            "body_text must contain U+2713 (checkmark) verbatim; got: " + bodyText);
        // The garbled Latin-1 misread form must NOT appear
        assertFalse(bodyText.contains("Â§"),
            "body_text must NOT contain the Latin-1 mojibake Â§; got: " + bodyText);
    }

    // =========================================================================
    // Discrimination: plain ASCII body produces no § in body_text
    // =========================================================================

    @Test
    void asciiOnlyBodyContainsNoSectionSign() {
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:sql-query", library = "jdbc")
              String query(String sql) {
                // purely ASCII comment
                return sql;
              }
            }
            """;

        JavaBindLifter.Result result = new JavaBindLifter().liftPathsFromSource("C.java", source);

        List<Jcs.Json> entries = sugarEntries(result);
        assertEquals(1, entries.size(), "expected one sugar entry");

        Jcs.Obj entry = (Jcs.Obj) entries.get(0);
        String bodyText = entry.objectField("body_source").stringField("body_text");

        assertFalse(bodyText.contains("§"),
            "ASCII-only body must not contain § (discrimination check)");
        assertFalse(bodyText.contains("Â"),
            "ASCII-only body must not contain Latin-1 prefix byte (discrimination check)");
    }

    // =========================================================================
    // Structural: body_source.body_text is always present on sugar entries
    // =========================================================================

    @Test
    void bodySourceBodyTextFieldAlwaysPresent() {
        String source = """
            package p;
            class C {
              @ProveKitSugar(concept = "concept:log-emit", library = "jul")
              void log(String msg) {
                System.out.println(msg);
              }
            }
            """;

        JavaBindLifter.Result result = new JavaBindLifter().liftPathsFromSource("C.java", source);

        List<Jcs.Json> entries = sugarEntries(result);
        assertEquals(1, entries.size(), "expected one sugar entry");

        Jcs.Obj entry = (Jcs.Obj) entries.get(0);
        Jcs.Json bodySourceJson = entry.get("body_source");
        assertNotNull(bodySourceJson,
            "body_source must be present on library-sugar-binding-entry (structural check)");
        assertTrue(bodySourceJson instanceof Jcs.Obj,
            "body_source must be a JSON object (structural check)");

        Jcs.Json bodyTextJson = ((Jcs.Obj) bodySourceJson).get("body_text");
        assertNotNull(bodyTextJson,
            "body_source.body_text must be present (structural check)");
        assertTrue(bodyTextJson instanceof Jcs.Str,
            "body_source.body_text must be a JSON string (structural check)");
    }

    private static List<Jcs.Json> sugarEntries(JavaBindLifter.Result result) {
        return result.entries().stream()
            .filter(e -> {
                if (!(e instanceof Jcs.Obj obj)) return false;
                return "library-sugar-binding-entry".equals(obj.stringFieldOrNull("kind"));
            })
            .toList();
    }
}
