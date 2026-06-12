package account;

import jakarta.validation.constraints.Pattern;

/**
 * Fixture vendor source for Door 3 @Pattern regex-universe walker tests.
 * Three @Pattern accessors exercising: (a) a regular pattern that registers,
 * (b) a non-regular pattern (backreference) refused by name, (c) a non-literal
 * regexp that is not walked.
 */
public final class Handles {

    // (a) REGULAR — registers; the regex literal is walked verbatim.
    @Pattern(regexp = "^[a-z][a-z0-9_]{2,15}$")
    public static String accept(String handle) {
        return handle;
    }

    // (b) NON-REGULAR — a backreference \1; must be REFUSED BY NAME (not registered).
    @Pattern(regexp = "(a)\\1")
    public static String risky(String s) {
        return s;
    }

    // (c) NON-LITERAL regexp — a constant reference, not a string literal AST node;
    //     the walker reads only LiteralTree<String>, so this is not registered.
    private static final String RX = "^[a-z]+$";
    @Pattern(regexp = RX)
    public static String dynamic(String s) {
        return s;
    }
}
