package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64String;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

/**
 * GOOD suite -- STRONG TIER mod-3 TAILS (paper 26 PHASE 2, "the encode universe
 * made total").
 *
 * The full-block strong tier (java-b64-strong) walks multiple-of-3 inputs. This
 * showcase closes the declared PHASE-2 gap: the mod-3 tails. The consumer
 * asserts the CORRECT standard-alphabet encodings of a 2-byte and a 1-byte
 * tail:
 *
 *     assertEquals("YmE=", encodeBase64String(getBytesUtf8("ba")))   // 2-byte tail
 *     assertEquals("Zg==", encodeBase64String(getBytesUtf8("f")))    // 1-byte tail
 *
 * The kit walks the vendor's EOF tail path (Base64.java:737-760) THROUGH THE
 * SAME symbolic interpreter as the full block:
 *
 *   2-byte tail (case 2, 16 bits = 6+6+4):
 *     work  = ((0 << 8) + b0) << 8) + b1
 *     out0  = table[(work >> 10) & MASK_6BITS]   // line 753
 *     out1  = table[(work >>  4) & MASK_6BITS]   // line 754
 *     out2  = table[(work <<  2) & MASK_6BITS]   // line 755
 *     pad   = '='                                // lines 757-758, STANDARD guard
 *
 *   1-byte tail (case 1, 8 bits = 6+2):
 *     work  = (0 << 8) + b0
 *     out0  = table[(work >> 2) & MASK_6BITS]    // line 742
 *     out1  = table[(work << 4) & MASK_6BITS]    // line 744
 *     pad   = '=' '='                            // lines 746-748, STANDARD guard
 *
 * The pad codepoint is NOT typed: it is resolved from the AST through the same
 * chain the weak tier uses (pad field <- ctor param <- super(...) arg <-
 * PAD_DEFAULT='='=61, BaseNCodec.java:179). The pad COUNT is the literal's
 * length mod 3 (a structural fact). The pad write is table-specific: the
 * vendor's own `if (encodeTable == STANDARD_ENCODE_TABLE)` guard -- urlsafe
 * skips it.
 *
 *   python3 -c "import base64; print(base64.b64encode(b'ba').decode())"  => YmE=
 *   python3 -c "import base64; print(base64.b64encode(b'f').decode())"   => Zg==
 */
public class B64TailsGoodTest {

    @Test
    public void testTwoByteTailDischargesCorrectClaim() {
        // 2-byte tail: 3 sextet chars + 1 '=' pad. Standard base64 of "ba" = "YmE=".
        assertEquals("YmE=", encodeBase64String(getBytesUtf8("ba")));
    }

    @Test
    public void testOneByteTailDischargesCorrectClaim() {
        // 1-byte tail: 2 sextet chars + 2 '=' pads. Standard base64 of "f" = "Zg==".
        assertEquals("Zg==", encodeBase64String(getBytesUtf8("f")));
    }
}
