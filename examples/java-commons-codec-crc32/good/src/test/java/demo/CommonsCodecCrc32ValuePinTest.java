package demo;

import org.apache.commons.codec.digest.PureJavaCrc32;
import org.junit.Test;

import static org.junit.Assert.assertEquals;

/**
 * GOOD suite: the standard CRC32 check value for the canonical input, exposed
 * through Apache Commons Codec's real PureJavaCrc32 byte-array update path.
 *
 * The vendor's own PureJavaCrc32Test warrants the implementation by comparing
 * PureJavaCrc32 against java.util.zip.CRC32 after both per-byte and byte-array
 * updates. This harness binds that vendor-warranted relation to the fixed
 * "123456789" check value.
 */
public class CommonsCodecCrc32ValuePinTest {
    @Test
    public void testCanonicalCheckValueWalked() {
        PureJavaCrc32 crc = new PureJavaCrc32();
        crc.update("123456789".getBytes(java.nio.charset.StandardCharsets.US_ASCII), 0, 9);
        long v = crc.getValue();
        assertEquals(0xCBF43926L, v);
    }
}
