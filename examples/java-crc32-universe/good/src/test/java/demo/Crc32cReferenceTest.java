package demo;

import java.util.zip.CRC32C;
import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * GOOD suite: the vendor's OWN sworn CRC-32C check value, lifted as a point
 * contract on the real vendor CRC API callsite.
 *
 * THE OATH IS THE VENDOR'S. OpenJDK's own test suite swears, verbatim:
 *   test/jdk/java/util/zip/TestCRC32C.java:
 *     ChecksumBase.testAll(new CRC32C(), 0xE3069283L);
 * where ChecksumBase (test/jdk/java/util/zip/ChecksumBase.java) feeds the
 * canonical check input "123456789" (US-ASCII) and asserts getValue()==expected.
 * 0xE3069283 is the CRC-32C (Castagnoli) analogue of the canonical CRC-32 check
 * value 0xCBF43926; it is the value the VENDOR SWORE, not one we computed.
 *
 * The callsite below is the vendor's REAL CRC32C API: update(byte[]) over the
 * canonical input, then getValue(). The result is bound to the SSA local `v`
 * and asserted to the vendor-sworn value. The kit substitutes `v` back to the
 * getValue() callsite and emits a location-keyed point contract scoped to this
 * test method → discharged (the sworn value is self-consistent).
 *
 * FLOOR scope (stated plainly): this proves the CRC value is the contract the
 * vendor SWORE (point equality on the real callsite). It does NOT DERIVE that
 * value from the table-generation recurrence + update loop. That derivation
 * tier is REFUSED BY NAME in PROVENANCE.md: CRC32C builds its table in a
 * `static {}` initializer (which the merged RecurrenceUniverseWalker, iterating
 * MethodTree members, does not enter) over a 2-D int[][] with `.length` bounds
 * and an `Integer.reverse(...)`-computed polynomial — every one a node the
 * keystone refuses by name rather than fake a connection to the oath.
 *
 * Logo: "OpenJDK's own CRC-32C check value, lifted from the vendor's real
 *        Checksum API and federated — the checksum's contract, sworn by the JDK."
 */
public class Crc32cReferenceTest {

    /** The canonical CRC check input, exactly as ChecksumBase feeds it. */
    private static final byte[] CHECK_INPUT = "123456789".getBytes();

    @Test
    public void testCanonicalCheckValue() {
        // Vendor-sworn: TestCRC32C.java → ChecksumBase.testAll(new CRC32C(), 0xE3069283L)
        CRC32C crc = new CRC32C();
        crc.update(CHECK_INPUT, 0, CHECK_INPUT.length);
        long v = crc.getValue();
        assertEquals(0xE3069283L, v);
    }
}
