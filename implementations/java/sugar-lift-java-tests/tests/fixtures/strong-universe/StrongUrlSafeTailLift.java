// Fixture: STRONG-TIER mod-3 TAIL on the URL-SAFE table. The vendor's pad write
// is guarded `if (encodeTable == STANDARD_ENCODE_TABLE)`, so the URL-SAFE table
// emits NO '=' pad. The strong tail must therefore emit the sextet equations
// with NO pad_chars -- table discipline, walked from the guard, never typed.
//
//   encodeBase64URLSafeString(getBytesUtf8("ba")) == "YmE"  (3 sextet, 0 pad)
import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64URLSafeString;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

public class StrongUrlSafeTailLift {

    @Test
    public void testUrlSafeTwoByteTailNoPad() {
        // URL-SAFE skips padding: the guard `encodeTable == STANDARD_ENCODE_TABLE`
        // is false for the urlsafe table, so no '=' is written.
        assertEquals("YmE", encodeBase64URLSafeString(getBytesUtf8("ba")));
    }
}
