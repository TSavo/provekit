package demo;

import java.util.zip.CRC32C;
import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * GOOD suite (VALUE-PIN tier): the vendor's OWN sworn CRC-32C check value,
 * DISCHARGED against the WALKED table+update computation — not a point contract,
 * and not a within-test contradiction.
 *
 * THE OATH IS THE VENDOR'S. OpenJDK's own test suite swears, verbatim:
 *   test/jdk/java/util/zip/TestCRC32C.java:
 *     ChecksumBase.testAll(new CRC32C(), 0xE3069283L);
 * where ChecksumBase feeds the canonical check input "123456789" (US-ASCII) and
 * asserts getValue()==expected. 0xE3069283 is the value the VENDOR SWORE.
 *
 * The receiver below is checksummed over EXACTLY the canonical input, one byte at
 * a time through the vendor's REAL `update(int b)`:
 *   crc = (crc >>> 8) ^ byteTable[(crc ^ (b & 0xFF)) & 0xFF]
 * then getValue() applies the final inversion ((~crc) & 0xFFFFFFFF).
 *
 * The kit WALKS that stateful update over the literal bytes, READING the FOLDED
 * static-init table (the merged construction-site walk), and emits a closed bv32
 * value-pin: crc("123456789") == 0xE3069283. The GOOD assertion DISCHARGES
 * against the walked crc-FOL — the vendor's sworn value is the value the walked
 * table+update computation produces.
 *
 * Logo: "OpenJDK's own CRC-32C check value, DERIVED from the vendor's real
 *        table-generation + stateful update by symbolic walk — the checksum's
 *        value, proven, not point-pinned."
 */
public class Crc32cValuePinTest {

    /** The canonical CRC check input, exactly as ChecksumBase feeds it. */
    private static final byte[] CHECK_INPUT = "123456789".getBytes();

    @Test
    public void testCanonicalCheckValueWalked() {
        CRC32C crc = new CRC32C();
        // Feed the canonical input byte-by-byte through the vendor's real update(int).
        crc.update('1');
        crc.update('2');
        crc.update('3');
        crc.update('4');
        crc.update('5');
        crc.update('6');
        crc.update('7');
        crc.update('8');
        crc.update('9');
        long v = crc.getValue();
        // Vendor-sworn: TestCRC32C.java → testAll(new CRC32C(), 0xE3069283L).
        // DISCHARGED against the walked table+update computation.
        assertEquals(0xE3069283L, v);
    }
}
