package com.provekit.runtime;

/**
 * Concept-identifying runtime helpers. Calls to these methods carry
 * substrate-canonical concept identity at runtime so the syntax-driven
 * lifter can recover the concept WITHOUT relying on citation comments.
 *
 * <p>The pattern: when the substrate lower produces a construct that's
 * either (a) a no-op the source explicitly named, or (b) a special
 * panic intended to be unreachable, or (c) a typed operation on a
 * carrier — the lower wraps it in a Substrate.X helper instead of
 * emitting raw java. Citation comments alongside become redundant
 * insurance, not the sole identity-bearer.
 *
 * <p>Each helper's name maps 1:1 to a canonical concept_name; the
 * lifter has a small fixed table.
 */
public final class Substrate {
    private Substrate() {}

    /** Identity that names the concept. Used for rust's .cloned() /
     *  .clone() / .into() lowered as identity in java (reference-shared
     *  semantics). Concept: {@code concept:value-clone}. */
    public static <T> T cloneOf(T value) {
        return value;
    }

    /** Typed unwrap on a Result carrier — same as result.unwrap() but
     *  the call site names the concept. Concept: {@code concept:try-unwrap}. */
    @SuppressWarnings("unchecked")
    public static <T, E> T tryUnwrap(Result<T, E> result) {
        return result.unwrap();
    }

    /** Panic for an unreachable branch — typically the synthesized
     *  else of an exhaustive match. Concept:
     *  {@code concept:exhaustive-match-no-default}. */
    public static <T> T unreachable(String message) {
        throw new RuntimeException("unreachable: " + message);
    }
}
