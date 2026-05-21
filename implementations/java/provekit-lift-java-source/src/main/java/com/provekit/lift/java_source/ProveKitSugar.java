// SPDX-License-Identifier: Apache-2.0
package com.provekit.lift.java_source;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Marks a method as a host-language sugar binding for a ProvekIt concept.
 * The JavaBindLifter recognizes this annotation and emits a
 * {@code library-sugar-binding-entry} record in the bind-IR output.
 *
 * <p>Three speech acts per paper 24:
 * <ol>
 *   <li>{@code @ProveKitSugar(concept=..., library=..., loss={})} -- materialize (exact)</li>
 *   <li>{@code @ProveKitSugar(concept=..., library=..., loss={"dim1","dim2"})} -- loudly-bounded-lossy</li>
 *   <li>{@code @ProveKitSugar(concept="concept:contract-observation", ..., observedDimension="...")} -- observation binding</li>
 * </ol>
 *
 * <p>Example:
 * <pre>{@code
 * @ProveKitSugar(concept = "concept:sql-execute", library = "sqlite-jdbc",
 *                loss = {"sync-vs-async", "last-insert-id"})
 * int execute(Connection conn, String sql) throws SQLException {
 *     return conn.createStatement().executeUpdate(sql);
 * }
 * }</pre>
 */
@Retention(RetentionPolicy.SOURCE)
@Target(ElementType.METHOD)
public @interface ProveKitSugar {
    /** The concept name, e.g. {@code "concept:http-request"}. Must be non-empty. */
    String concept();

    /** The library tag, e.g. {@code "java-net-http"}. Must be non-empty. */
    String library();

    /**
     * Loss dimensions declared for this binding (paper 24 §3).
     * Empty array means exact materialization; non-empty means loudly-bounded-lossy.
     * Each entry is a dimension slug, e.g. {@code "sync-vs-async"}.
     */
    String[] loss() default {};

    /**
     * For {@code concept:contract-observation} bindings: the dimension being observed,
     * e.g. {@code "autocommit-mode"}. Empty string means not an observation binding.
     */
    String observedDimension() default "";

    /**
     * #1357 / #1355: optional concept family pin, e.g. {@code "concept:family:sql"}.
     * Empty string means the family axis FLOATS — the dispatcher narrows
     * via the manifest's family or the project's [platform_profile] at
     * materialize time. Parallel to walk_rpc (rust) + typescript-source +
     * python-source lifters.
     */
    String family() default "";

    /**
     * #1357 / #1355: optional library version pin, e.g. {@code "3.45.3.0"}.
     * Empty string means the version axis FLOATS. Parallel to family().
     */
    String version() default "";
}
