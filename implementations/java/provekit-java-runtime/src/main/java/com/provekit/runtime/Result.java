package com.provekit.runtime;

import java.util.function.Function;
import java.util.function.Supplier;

/**
 * Substrate-runtime carrier for rust's {@code Result<T, E>} (and similar
 * fallible-value sum types in other source languages).
 *
 * <p>The substrate concept this carrier realizes is {@code concept:fallible-value}
 * (sum of {@code concept:fallible-ok} + {@code concept:fallible-err}). Code lowered
 * from a source language that has native Result-like semantics references
 * this type so the (B) functional-opacity side of substrate-honest transport
 * is preserved: callers can still do .isOk() / .isErr() / .map() / .mapErr()
 * the way the source language allowed.
 *
 * <p>Citation comments in the lowered source supply (A) round-trip
 * recoverability — together with this carrier the cycle source → ProofIR
 * → java → ProofIR → source is lossless.
 */
public sealed interface Result<T, E> permits Result.Ok, Result.Err {

    record Ok<T, E>(T value) implements Result<T, E> {}

    record Err<T, E>(E error) implements Result<T, E> {}

    static <T, E> Result<T, E> ok(T value) {
        return new Ok<>(value);
    }

    static <T, E> Result<T, E> err(E error) {
        return new Err<>(error);
    }

    /** Convert {@code Option<T>}-as-nullable to {@code Result<T, E>} by
     *  supplying the error producer when the value is null. Mirrors
     *  rust's Option::ok_or_else. */
    static <T, E> Result<T, E> okOrElse(T valueOrNull, Supplier<E> errorIfNull) {
        return valueOrNull != null ? ok(valueOrNull) : err(errorIfNull.get());
    }

    default boolean isOk() {
        return this instanceof Ok<T, E>;
    }

    default boolean isErr() {
        return this instanceof Err<T, E>;
    }

    /** Get the value or throw a RuntimeException if Err. Mirrors rust unwrap. */
    @SuppressWarnings("unchecked")
    default T unwrap() {
        if (this instanceof Ok<T, E> ok) return ok.value;
        Err<T, E> err = (Err<T, E>) this;
        throw new RuntimeException("unwrap on Err: " + err.error);
    }

    /** Get the value or a fallback if Err. Mirrors rust unwrap_or. */
    @SuppressWarnings("unchecked")
    default T unwrapOr(T fallback) {
        return this instanceof Ok<T, E> ok ? ok.value : fallback;
    }

    /** Get the Err's error or throw if this is Ok. Mirrors rust unwrap_err. */
    @SuppressWarnings("unchecked")
    default E unwrapErr() {
        if (this instanceof Err<T, E> err) return err.error;
        Ok<T, E> ok = (Ok<T, E>) this;
        throw new RuntimeException("unwrap_err on Ok: " + ok.value);
    }

    /** Transform Ok's value via mapper; pass Err through unchanged. */
    @SuppressWarnings("unchecked")
    default <U> Result<U, E> map(Function<T, U> mapper) {
        if (this instanceof Ok<T, E> ok) {
            return Result.ok(mapper.apply(ok.value));
        }
        return (Result<U, E>) this;
    }

    /** Transform Err's value via mapper; pass Ok through unchanged. */
    @SuppressWarnings("unchecked")
    default <F> Result<T, F> mapErr(Function<E, F> mapper) {
        if (this instanceof Err<T, E> err) {
            return Result.err(mapper.apply(err.error));
        }
        return (Result<T, F>) this;
    }
}
