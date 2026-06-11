package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64String;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

/**
 * BAD suite: the URL-safe confusion marquee.
 *
 * A consumer calls the STANDARD encode method -- encodeBase64String -- but
 * asserts a URL-SAFE alphabet expected value. The expected string
 * "cHJvdmVraXR-c2VhbQ==" contains '-', which is the URL-safe replacement for
 * '+'. The character '-' is NOT a member of the STANDARD_ENCODE_TABLE walked
 * from vendor/commons-codec/Base64.java (tag rel/commons-codec-1.16.1).
 *
 * commons-codec's test suite has no assertion for encodeBase64String over
 * "provekit~seam". There is no vendor-sworn point vector to conjoin against.
 * In the point-contract era this claim is unconstrained -- no collision, no
 * refutation. Under universe contracts:
 *
 *     str.chars-in-set(encodeBase64String("provekit~seam"), <walked STANDARD_ENCODE_TABLE>)
 *
 * conjoins with this equality under the same #euf# contract name. The
 * STANDARD_ENCODE_TABLE does not contain '-'. The conjunction is UNSAT ->
 * unsatisfied. The refutation comes entirely from the universe walked from
 * the vendor's source, gated by their own samples -- the vendor never tested
 * this input; the universe's, not a point's, is the refutation.
 *
 * The correct standard encoding is "cHJvdmVraXR+c2VhbQ==" ('+' not '-').
 * python3: base64.b64encode(b'provekit~seam').decode() == 'cHJvdmVraXR+c2VhbQ=='
 * python3: base64.urlsafe_b64encode(b'provekit~seam').decode() == 'cHJvdmVraXR-c2VhbQ=='
 *
 * This is the marquee of paper 26: the bad twin asserts the URL-safe
 * confusion on an input the vendor never tested, and the real CLI returns
 * unsatisfied.
 */
public class UrlSafeSeamBadTest {

    @Test
    public void testUrlSafeConfusionOnUntestedInput() {
        // FALSE: standard base64 never emits '-' (URL-safe char).
        // The standard encoding is "cHJvdmVraXR+c2VhbQ==" -- '+' not '-'.
        // The vendor never tested "provekit~seam" -- the universe refutes it anyway.
        assertEquals("cHJvdmVraXR-c2VhbQ==", encodeBase64String(getBytesUtf8("provekit~seam")));
    }
}
