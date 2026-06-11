import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * BAD suite: single assertion that contradicts the two-layer construction fact.
 *
 * Wrapper is constructed with new Wrapper(new Box(5)), which pins:
 *   w.unwrap().get() == 5   (by two-layer Voltron construction walk)
 *
 * This test asserts assertEquals(6, w.unwrap().get()) — claims the result is 6.
 *
 * The kit emits TWO operands in the contract's `and`:
 *   operand[0] = construction fact: =(get(w.unwrap__), 5)   [from Box.java ctor, via Wrapper]
 *   operand[1] = test's claim:      =(get(w.unwrap__), 6)   [from this assertion]
 *
 * The solver sees =(t,5) AND =(t,6) — t cannot be both 5 and 6 → UNSATISFIED.
 *
 * NOTE: this is a SINGLE assertion with NO internal contradiction. Without the
 * two-layer construction operand it would wrongly discharge (opaque term equals anything).
 * The refutation comes solely from Box's constructor pinning the value through Wrapper.
 */
public class WrapperVoltronTest {

    @Test
    public void testTwoLayerChainWrongValue() {
        Wrapper w = new Wrapper(new Box(5));
        assertEquals(6, w.unwrap().get());
    }
}
