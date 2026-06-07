// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-stdio-java: java.io's @ProveKitSugar shim.
//
// Realizes stdio line-stream concepts via the JDK standard library
// (System.in / System.out / System.err). Sister shim to:
//   - rust: provekit-shim-stdio-rust (std::io)
//   - python: provekit-shim-stdio-python (sys.stdin / sys.stdout)
// All members of concept:family:stdio-stream, anchored to
// boundary:stdio-line-stream.
//
// Substrate-honest concept naming: the 1:1 alignment with provekit-shim-stdio-rust
// is the cross-library cluster signal — same concept name across all kits in
// the family. Materialize routes (concept, target_lang) via the catalog.

package org.provekit.shim.stdio_java;

import com.provekit.lift.java_source.ProveKitSugar;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.UncheckedIOException;

/**
 * Java realizations of concept:family:stdio-stream concepts.
 * <p>
 * Three speech acts per paper 24:
 * <ol>
 *   <li>{@code @ProveKitSugar(... loss = {})} -- materialize (exact)</li>
 *   <li>{@code @ProveKitSugar(... loss = {"<dim>"})} -- loudly-bounded-lossy</li>
 * </ol>
 * <p>
 * Concept names are vendored under this kit's signature, matching
 * provekit-shim-stdio-rust 1:1 where the substrate cluster membership applies.
 */
public final class StdioJavaShim {

    private StdioJavaShim() {
        // Utility class — instantiation forbidden.
    }

    /**
     * Reader used for the line-buffered read path. JDK convention is to wrap
     * System.in once and reuse — re-creating a BufferedReader per call would
     * lose pre-buffered bytes when the underlying stream blocks. The static
     * holder mirrors that convention.
     */
    private static final BufferedReader STDIN_READER =
        new BufferedReader(new InputStreamReader(System.in));

    /**
     * {@code concept:stdio-read-line} — reads one line from stdin
     * (without the trailing newline). Returns {@code null} at EOF.
     * <p>
     * Mirrors {@code provekit-shim-stdio-rust::stdin_read_line} which returns
     * {@code Option<String>::None} at EOF; java's null is the substrate-honest
     * realization of the absent value here (cluster-aligned via the rust:Null
     * morphism declared in the rust:Null → concept:Null catalog entry).
     */
    @ProveKitSugar(
        concept = "concept:stdio-read-line",
        library = "java-io",
        family = "concept:family:stdio-stream",
        version = "jdk-1.0+",
        loss = {}
    )
    public static String stdin_read_line() {
        try {
            return STDIN_READER.readLine();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    /**
     * {@code concept:stdio-write-line} — writes one line to stdout, appending
     * a newline. Mirrors {@code provekit-shim-stdio-rust::stdout_write_line}.
     */
    @ProveKitSugar(
        concept = "concept:stdio-write-line",
        library = "java-io",
        family = "concept:family:stdio-stream",
        version = "jdk-1.0+",
        loss = {}
    )
    public static void stdout_write_line(String line) {
        System.out.println(line);
    }

    /**
     * {@code concept:stderr-write-line} — writes one line to stderr, appending
     * a newline. Mirrors {@code provekit-shim-stdio-rust::stderr_write_line}.
     */
    @ProveKitSugar(
        concept = "concept:stderr-write-line",
        library = "java-io",
        family = "concept:family:stdio-stream",
        version = "jdk-1.0+",
        loss = {}
    )
    public static void stderr_write_line(String line) {
        System.err.println(line);
    }
}
