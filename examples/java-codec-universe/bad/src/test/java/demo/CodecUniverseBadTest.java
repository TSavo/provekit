package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64URLSafeString;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

/**
 * BAD suite: a consumer's FALSE claim about an input the vendor NEVER tested.
 *
 * commons-codec's test suite (rel/commons-codec-1.16.1) has no assertion for
 * encodeBase64URLSafeString over "bar". There is no sworn vector to conjoin
 * against — point-wise consistency alone could never catch this claim.
 *
 * The claimed output "YmFy+/x=" contains '+', '/', and '=' — none of which
 * are members of the URL_SAFE_ENCODE_TABLE walked from the vendor's source
 * ('-' and '_' replace '+' and '/'; the vendor's own pad-write guard
 * `if (encodeTable == STANDARD_ENCODE_TABLE)` attributes the pad char to the
 * standard table ONLY). The universe row
 *
 *     str.chars-in-set(encodeBase64URLSafeString("bar"), <walked URL-safe table>)
 *
 * conjoins with this equality under the same #euf# contract name, and z3's
 * string theory refutes it: UNSAT. The refutation comes from the universe
 * walked from the vendor's source, gated by their samples — not from any
 * vendor-tested vector.
 */
public class CodecUniverseBadTest {

    @Test
    public void testUrlSafeConfusionOnUntestedInput() {
        // FALSE: url-safe base64 never emits '+', '/', or pad '='.
        // The vendor never tested "bar" — the universe refutes it anyway.
        assertEquals("YmFy+/x=", encodeBase64URLSafeString(getBytesUtf8("bar")));
    }
}
