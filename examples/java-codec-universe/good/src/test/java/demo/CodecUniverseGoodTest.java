package demo;

import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64String;
import static org.apache.commons.codec.binary.StringUtils.getBytesUtf8;

/**
 * GOOD suite: the vendor's OWN sworn vector.
 *
 * This is the verbatim RFC 4648 section-10 assertion from commons-codec's
 * own test suite — Base64Test.java:878 (tag rel/commons-codec-1.16.1):
 *
 *     assertEquals("Zm9v", Base64.encodeBase64String(StringUtils.getBytesUtf8("foo")));
 *
 * The kit lifts TWO contracts with the SAME #euf# name for this callsite:
 *   1. the sworn equality   =(encodeBase64String("foo"), "Zm9v")
 *   2. the universe row     str.chars-in-set(encodeBase64String("foo"), <walked table>)
 * The walked table is the STANDARD_ENCODE_TABLE ∪ pad, every character of
 * which traces to a LiteralTree node in vendor/commons-codec/Base64.java
 * (the urlSafe=false chain is walked by literal propagation, never assumed).
 *
 * The conjunction is satisfiable iff every character of "Zm9v" is a member
 * of the walked table — the vendor's own sample GATES our reading of their
 * implementation. A wrong universe would refute the vendor's own test.
 */
public class CodecUniverseGoodTest {

    @Test
    public void testRfc4648FooVector() {
        // Vendor-sworn: commons-codec Base64Test.java:878 (rel/commons-codec-1.16.1)
        assertEquals("Zm9v", encodeBase64String(getBytesUtf8("foo")));
    }
}
