package demo;

import org.apache.commons.codec.digest.PureJavaCrc32;
import org.junit.Test;

import static org.junit.Assert.assertEquals;

/**
 * BAD suite: one wrong CRC32 value over the same vendor-warranted byte-array
 * update path. The refutation must come from the walked slicing-by-8 table
 * relation, not from a sibling contradiction.
 */
public class CommonsCodecCrc32WrongValueTest {
    @Test
    public void testWrongValueRefutedByWalk() {
        PureJavaCrc32 crc = new PureJavaCrc32();
        crc.update("123456789".getBytes(java.nio.charset.StandardCharsets.US_ASCII), 0, 9);
        long v = crc.getValue();
        assertEquals(0xCBF43927L, v);
    }
}
