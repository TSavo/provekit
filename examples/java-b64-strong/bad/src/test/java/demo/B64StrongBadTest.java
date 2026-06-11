package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64String;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

/**
 * BAD suite -- THE STRONG-TIER MARQUEE (paper 26, "the moment the universe
 * stops being a set and becomes a function").
 *
 * The consumer asserts a WRONG output for encode("bar"):
 *
 *     assertEquals("ZmFy", encodeBase64String(getBytesUtf8("bar")))
 *
 * "ZmFy" is the encoding of "foo", not "bar". The correct value is "YmFy".
 *
 * THE POINT: "ZmFy" is ALPHABET-VALID. Every character (Z, m, F, y) IS a
 * member of the STANDARD_ENCODE_TABLE. So the WEAK tier (str.chars-in-set:
 * every output char is in the table) DISCHARGES this claim -- the weak
 * universe is a SET, and "ZmFy" is inside it. The weak tier cannot see the
 * bug.
 *
 * Only the strong tier refutes it. The block equations PIN the output as a
 * FUNCTION of the input bytes:
 *
 *     out0 = table[(work >> 18) & 0x3f]  with work from b='b',a='a',r='r'
 *          = table[24] = 'Y'   (NOT 'Z' = table[25])
 *
 * The sworn equality (out == "ZmFy") conjoined with the block-equation
 * universe (out == "YmFy", computed by z3 from the walked body) is a
 * contradiction: UNSAT -> unsatisfied.
 *
 *   python3 -c "import base64; print(base64.b64encode(b'bar').decode())"
 *     => YmFy   (not ZmFy)
 *
 * This refutation is the entire reason the strong tier exists.
 */
public class B64StrongBadTest {

    @Test
    public void testStrongBlockEquationsRefuteAlphabetValidButWrongClaim() {
        // FALSE: "ZmFy" is encode("foo"), not encode("bar"). It is alphabet-valid
        // (every char in the standard table) so the weak tier discharges it.
        // Only the block equations refute it: encode("bar") == "YmFy".
        assertEquals("ZmFy", encodeBase64String(getBytesUtf8("bar")));
    }
}
