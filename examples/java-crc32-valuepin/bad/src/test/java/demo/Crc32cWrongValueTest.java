package demo;

import java.util.zip.CRC32C;
import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * BAD suite (VALUE-PIN tier): a SINGLE wrong CRC value, refuted UNSATISFIED BY
 * THE WALKED TABLE+UPDATE COMPUTATION — NOT by a within-test contradiction.
 *
 * The receiver is checksummed over EXACTLY the canonical input "123456789" via
 * the vendor's real `update(int b)`, then getValue() is asserted to a WRONG
 * value: 0xE3069284 (the sworn value 0xE3069283 off by one). There is exactly
 * ONE assertion. It is not contradicted by any sibling assertion.
 *
 * The kit emits the value-pin contract:
 *   crc32.eq-walked( 0xE3069284 , <walked crc-FOL for "123456789"> )
 * whose inv is the single equation
 *   (= #xe3069284 <walked table+update+inversion FOL>)
 * The walked RHS constant-folds (in z3) to the genuine 0xE3069283, so the
 * equation is UNSAT → unsatisfied. The refutation is EQUATION-DRIVEN: the
 * universe (the folded table + the walked stateful update) does the work, like
 * java-b64-strong refuting "ZmFy". It is distinct from the floor showcase's
 * within-test contradiction (two assertions on one callsite).
 */
public class Crc32cWrongValueTest {

    @Test
    public void testWrongValueRefutedByWalk() {
        CRC32C crc = new CRC32C();
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
        // ONE assertion, a WRONG value. The sworn value is 0xE3069283; this is
        // off by one. Refuted by the walked table+update computation, not by a
        // contradicting sibling assertion.
        assertEquals(0xE3069284L, v);
    }
}
