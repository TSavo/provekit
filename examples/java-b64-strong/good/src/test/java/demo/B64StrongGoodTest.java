package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64String;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

/**
 * GOOD suite -- STRONG TIER (paper 26, "THE seam between tiers").
 *
 * The consumer asserts the CORRECT standard-alphabet encoding of "bar":
 *
 *     assertEquals("YmFy", encodeBase64String(getBytesUtf8("bar")))
 *
 * "bar" is exactly one full 3-byte block (no mod-3 tail). The kit walks the
 * vendor's encode body (Base64.java:778-783) and mints the PER-CHARACTER
 * EQUATIONS as a strong-tier universe:
 *
 *     work  = ((((0 << 8) + b0) << 8) + b1) << 8) + b2     // accumulation, line 778
 *     out0  = table[(work >> 18) & MASK_6BITS]              // line 780
 *     out1  = table[(work >> 12) & MASK_6BITS]              // line 781
 *     out2  = table[(work >>  6) & MASK_6BITS]              // line 782
 *     out3  = table[ work        & MASK_6BITS]              // line 783
 *
 * Every shift (18/12/6), the 6-bit mask (MASK_6BITS = 0x3f, line 129), the
 * accumulation shift (8) and the 64 table codepoints trace to a com.sun.source
 * tree node of the vendored source. Nothing is hand-authored.
 *
 * z3 computes the equations: with b0,b1,b2 = 98,97,114 ('b','a','r') the
 * output is "YmFy", byte-for-byte. The sworn equality and the block-equation
 * universe conjoin under the SAME #euf# name: SAT -> discharged.
 *
 *   python3 -c "import base64; print(base64.b64encode(b'bar').decode())"
 *     => YmFy
 */
public class B64StrongGoodTest {

    @Test
    public void testStrongBlockEquationsDischargeCorrectClaim() {
        // Correct: standard base64 of "bar" is "YmFy".
        // The block equations COMPUTE this from the walked encode body.
        assertEquals("YmFy", encodeBase64String(getBytesUtf8("bar")));
    }
}
