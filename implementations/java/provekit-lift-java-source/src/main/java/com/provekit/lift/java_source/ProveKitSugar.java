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
 * <p>Example:
 * <pre>{@code
 * @ProveKitSugar(concept = "concept:http-request", library = "java-net-http")
 * int fetchStatus(URI uri) {
 *     return HttpClient.newHttpClient()
 *         .send(HttpRequest.newBuilder(uri).build(), BodyHandlers.discarding())
 *         .statusCode();
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
}
