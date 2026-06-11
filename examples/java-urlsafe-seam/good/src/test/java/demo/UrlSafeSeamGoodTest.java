package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64String;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

/**
 * GOOD suite: consumer asserts the CORRECT standard-alphabet encoding of
 * an input the vendor NEVER tested -- "provekit~seam".
 *
 * commons-codec's test suite (rel/commons-codec-1.16.1) has no assertion for
 * encodeBase64String over "provekit~seam". There is no vendor-sworn point
 * vector to conjoin against. Yet the universe row PINS the output space:
 * every character of the result must be a member of the walked
 * STANDARD_ENCODE_TABLE (A-Za-z0-9+/=).
 *
 * python3 -c "import base64; print(base64.b64encode(b'provekit~seam').decode())"
 *   => cHJvdmVraXR+c2VhbQ==
 *
 * All characters of "cHJvdmVraXR+c2VhbQ==" are in the standard alphabet
 * (A-Za-z0-9+/=). The equality row and universe row conjoin consistently:
 * SAT -> discharged.
 *
 * The standard encoding CONTAINS '+' (at position 12). This is the seam
 * input: a consumer who mistakes standard for URL-safe would write the
 * URL-safe spelling ("cHJvdmVraXR-c2VhbQ==", '-' replacing '+'), which is
 * the confusion the BAD suite demonstrates.
 */
public class UrlSafeSeamGoodTest {

    @Test
    public void testStandardEncodingOnUntestedInput() {
        // Correct: standard base64 of "provekit~seam" is "cHJvdmVraXR+c2VhbQ=="
        // python3: base64.b64encode(b'provekit~seam').decode() == 'cHJvdmVraXR+c2VhbQ=='
        // All chars in STANDARD_ENCODE_TABLE (A-Za-z0-9+/=) -- universe consistent.
        assertEquals("cHJvdmVraXR+c2VhbQ==", encodeBase64String(getBytesUtf8("provekit~seam")));
    }
}
