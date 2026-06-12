package account;

import jakarta.validation.constraints.Pattern;

/**
 * A validated user handle, exactly as it arrives off the wire. The Bean
 * Validation (JSR-380) {@code @Pattern} annotation is the contract: it decides
 * whether a handle string is acceptable, and the rule lives here, in the source,
 * next to the accessor it governs.
 *
 * THE OATH IS THE VENDOR'S. The regular expression below is the verbatim
 * {@code @Pattern(regexp="…")} literal; the kit walks it from THIS annotation's
 * AST node into the regular language it denotes. The author's intent — written
 * in the comment — is "a lowercase handle: starts with a letter, then 2 to 15
 * of letter / digit / underscore." Whether the WRITTEN regex actually pins that
 * intent is exactly what z3 decides over the walked language.
 *
 * The validating accessor is {@code static String accept(String)} so the
 * call-site identity keys on the literal input (EUF-federated), exactly the
 * shape the kit's string-equality universe path recognises — the same shape as
 * a static codec encode entry point.
 */
public final class UserHandle {

    /**
     * The validated handle accessor. Returns the input unchanged when it is a
     * member of the {@code @Pattern} language; the annotation IS the contract on
     * the returned value. A consumer's claim that {@code accept(x)} equals a
     * particular string is, in the validated world, a claim that the string is a
     * member of the {@code @Pattern} language — the kit conjoins exactly that
     * membership obligation under the same EUF name.
     *
     * Intended: lowercase handle, 3..16 chars, letter-led, [a-z0-9_] body.
     * Walked verbatim into a regular language and pinned as a str.in-regex
     * universe row over accept()'s callresult.
     */
    @Pattern(regexp = "^[a-z][a-z0-9_]{2,15}$")
    public static String accept(String handle) {
        return handle;
    }
}
