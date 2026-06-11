// Fixture: non-multiple-of-3 input -> the mod-3 tail is PHASE 2, REFUSED BY
// NAME. The weak str.chars-in-set row STILL emits; the strong row does not.
import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64String;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

public class StrongTailRefusal {

    @Test
    public void testTwoByteTailRefused() {
        // "ba" = 2 bytes (mod-3 tail). Standard b64 = "YmE=". The tail (+'=' pad)
        // is not yet walked: strong row refused by name, weak row stands.
        assertEquals("YmE=", encodeBase64String(getBytesUtf8("ba")));
    }
}
