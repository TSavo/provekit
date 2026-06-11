package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64String;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

/**
 * BAD suite -- THE STRONG-TIER TAIL MARQUEE (paper 26 PHASE 2).
 *
 * The consumer asserts a WRONG output for the 2-byte tail encode("ba"):
 *
 *     assertEquals("YmX=", encodeBase64String(getBytesUtf8("ba")))
 *
 * The correct value is "YmE=". "YmX=" differs only in the third character
 * (X instead of E).
 *
 * THE POINT: "YmX=" is ALPHABET-VALID. Every character is legal:
 *   - Y, m, X are all members of STANDARD_ENCODE_TABLE.
 *   - '=' is the sworn pad char (the weak universe includes it for the
 *     standard table, walked from the vendor's own pad guard).
 * So the WEAK tier (str.chars-in-set: every output char is in table ∪ {pad})
 * DISCHARGES this claim. The weak tier cannot see the bug -- the lie lives
 * INSIDE the alphabet.
 *
 * Only the strong TAIL equations refute it. They pin the third char as a
 * FUNCTION of the leftover bytes (Base64.java:755):
 *
 *     out2 = table[(work << 2) & 0x3f]  with work from b0='b', b1='a'
 *          = table[4] = 'E'   (NOT 'X' = table[23])
 *
 * The sworn equality (out == "YmX=") conjoined with the tail-equation universe
 * (out == "YmE=", computed by z3 from the walked tail body + the AST-resolved
 * pad) is a contradiction: UNSAT -> unsatisfied.
 *
 *   python3 -c "import base64; print(base64.b64encode(b'ba').decode())"
 *     => YmE=   (not YmX=)
 *
 * This refutation -- of a PADDED, alphabet-valid lie -- is the entire reason
 * the tail walk exists.
 */
public class B64TailsBadTest {

    @Test
    public void testTailEquationsRefuteAlphabetValidButWrongPaddedClaim() {
        // FALSE: standard base64 of "ba" is "YmE=", not "YmX=". "YmX=" is
        // alphabet-valid (Y,m,X in table; '=' is the sworn pad) so the weak tier
        // discharges it. Only the tail sextet equations refute it.
        assertEquals("YmX=", encodeBase64String(getBytesUtf8("ba")));
    }
}
