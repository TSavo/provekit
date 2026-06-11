package demo;

import java.util.zip.CRC32C;
import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * BAD suite: within-test contradiction on the SAME CRC-32C callsite.
 *
 * The canonical input "123456789" is checksummed once with the vendor's real
 * CRC32C API; the result is bound to the SSA local `v`. Two CONTRADICTORY
 * assertions are then made about `v`:
 *
 *   assertEquals(0xE3069283L, v);   // the JDK-sworn correct value
 *                                   //   (TestCRC32C.java: testAll(new CRC32C(), 0xE3069283L))
 *   assertEquals(0x12345678L, v);   // a WRONG CRC value — contradicts the line above
 *
 * The kit emits TWO point contracts for the SAME getValue() callsite (same
 * receiver `crc`, same SSA local `v`, same location key):
 *
 *   =(getValue(crc), 0xE3069283)   — from the first assertEquals
 *   =(getValue(crc), 0x12345678)   — from the second assertEquals
 *
 * The location key conjoins both: one value cannot equal two distinct integers
 * → UNSAT → unsatisfied. The refutation is the contract conjunction (the same
 * mechanism as java-mt-reference/bad and java-callbind-consistency/bad), NOT a
 * derivation of the correct output from the table-gen recurrence. The
 * derivation tier (which would refute a lone plausible-but-wrong value WITHOUT
 * a second contradicting assertion) is REFUSED BY NAME — see PROVENANCE.md: the
 * CRC32C table is generated in a `static {}` initializer the merged
 * RecurrenceUniverseWalker does not enter.
 */
public class Crc32cContradictionTest {

    @Test
    public void testContradiction() {
        CRC32C crc = new CRC32C();
        byte[] input = "123456789".getBytes();
        crc.update(input, 0, input.length);

        // result bound to SSA local `v`
        long v = crc.getValue();

        // Two contradictory assertions about the SAME checksum via SSA local `v`.
        assertEquals(0xE3069283L, v);   // JDK-sworn correct value
        assertEquals(0x12345678L, v);   // wrong CRC — contradicts the line above
    }
}
