package demo;

import org.apache.commons.codec.digest.PureJavaCrc32;
import org.junit.Test;

import static org.junit.Assert.assertEquals;

public class CommonsCodecCrc32Test {
    @Test
    public void checksumOfCanonicalInput() {
        PureJavaCrc32 crc = new PureJavaCrc32();
        crc.update("123456789".getBytes(java.nio.charset.StandardCharsets.US_ASCII), 0, 9);
        long v = crc.getValue();
        assertEquals(0xCBF43926L, v);
    }
}
