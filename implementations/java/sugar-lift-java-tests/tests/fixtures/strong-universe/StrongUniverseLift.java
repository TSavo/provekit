// Fixture: STRONG-TIER (str.eq-bv-blocks) walk over the real commons-codec
// Base64.java. A string-literal input of length a multiple of 3 (one full
// block, "bar") lifts THREE rows under the SAME #euf# name: the sworn equality,
// the weak str.chars-in-set universe, AND the strong per-character block
// equations walked by symbolic execution of the encode body.
import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64String;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

public class StrongUniverseLift {

    @Test
    public void testFullBlockStrong() {
        // "bar" = one full 3-byte block, no mod-3 tail. Standard b64 = "YmFy".
        // The strong walker emits 4 per-character index equations over b0,b1,b2.
        assertEquals("YmFy", encodeBase64String(getBytesUtf8("bar")));
    }
}
