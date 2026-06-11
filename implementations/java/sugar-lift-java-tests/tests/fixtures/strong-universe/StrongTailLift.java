// Fixture: STRONG-TIER mod-3 TAILS (PHASE 2). A non-multiple-of-3 literal now
// lifts the full blocks PLUS the walked tail sextet equations and the AST-
// resolved '=' pad chars, all under the SAME #euf# name as the sworn equality
// and the weak str.chars-in-set universe.
//
//   "ba" = 2-byte tail  -> standard b64 "YmE="  (3 sextet chars + 1 pad)
//   "f"  = 1-byte tail  -> standard b64 "Zg=="  (2 sextet chars + 2 pads)
import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64String;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

public class StrongTailLift {

    @Test
    public void testTwoByteTail() {
        // 2-byte tail: 3 sextet chars over (b0,b1) + 1 '=' pad. Standard "YmE=".
        assertEquals("YmE=", encodeBase64String(getBytesUtf8("ba")));
    }

    @Test
    public void testOneByteTail() {
        // 1-byte tail: 2 sextet chars over (b0) + 2 '=' pads. Standard "Zg==".
        assertEquals("Zg==", encodeBase64String(getBytesUtf8("f")));
    }
}
