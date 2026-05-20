// SPDX-License-Identifier: Apache-2.0
package com.provekit.lift.java_source;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks an empty marker class as a refused concept boundary for a ProvekIt kit.
 * The JavaBindLifter recognizes this annotation and emits a
 * {@code refusal-memento} record in the bind-IR output, which cmd_mint
 * signs as a {@code RefusalMemento} envelope member.
 *
 * <p>Each refusal is a signed signpost: the substrate publishes the demand the
 * shim declines to fill, naming the cluster constraint that would close it.
 * Mirrors {@code #[provekit::refuse(...)]} in the Rust surface model.
 *
 * <p>Place on an empty class (not a method), analogous to Rust module markers:
 * <pre>{@code
 * @ProveKitRefuse(
 *     surface = "java.sql.Connection#nativeSQL",
 *     concept = "concept:sql-native-sql",
 *     reason = "Driver-specific native SQL translation. No cross-library analog.",
 *     wouldCloseWithCluster = "Native SQL translation on >=2 JDBC drivers"
 * )
 * static final class RefusedNativeSql {}
 * }</pre>
 */
@Retention(RetentionPolicy.SOURCE)
@Target(ElementType.TYPE)
public @interface ProveKitRefuse {
    /**
     * The library surface being refused, e.g. {@code "java.sql.Connection#backup"}.
     * Mirrors the {@code surface} field in {@code #[provekit::refuse]}.
     */
    String surface();

    /**
     * The concept name that would have been bound, e.g. {@code "concept:sql-physical-backup"}.
     */
    String concept();

    /**
     * Human-readable explanation of why this surface is refused.
     * Should name the cross-library cluster constraint that prevents binding.
     */
    String reason();

    /**
     * What would need to be true for a future kit to close the cluster
     * (i.e., ship a binding here), e.g. {@code "Physical backup on >=2 SQL drivers"}.
     * Mirrors {@code would_close_with_cluster} in the Rust counterpart.
     */
    String wouldCloseWithCluster();
}
