package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static account.UserHandle.accept;

/**
 * GOOD suite: a consumer's validity claim about an input the @Pattern ACCEPTS.
 *
 * The vendor's contract is the @Pattern annotation on UserHandle.accept:
 *
 *     @Pattern(regexp = "^[a-z][a-z0-9_]{2,15}$")
 *     public static String accept(String handle) { return handle; }
 *
 * The kit lifts TWO contracts with the SAME #euf# name for this callsite:
 *   1. the sworn equality   =(accept("alice_01"), "alice_01")
 *   2. the regex universe   str.in-regex(accept("alice_01"), <walked @Pattern regex>)
 * The regex is parsed from the @Pattern annotation's AST literal and lowered to
 * z3's native RegLan theory (str.in_re / re.range / re.++ / re.loop / re.union).
 *
 * The conjunction is satisfiable iff "alice_01" is a member of the walked
 * regular language — which it is: an 'a'-led, 8-character handle of [a-z0-9_].
 * SAT → discharged. The consumer's claim is consistent with the vendor's
 * walked @Pattern contract.
 */
public class PatternRegexGoodTest {

    @Test
    public void testAcceptedHandleIsValid() {
        // The @Pattern language accepts this handle: letter-led, 8 chars, [a-z0-9_].
        assertEquals("alice_01", accept("alice_01"));
    }
}
