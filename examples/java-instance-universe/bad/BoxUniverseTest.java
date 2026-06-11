import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * BAD suite: single assertion that contradicts the construction fact.
 *
 * Box is constructed with new Box(5), which pins x.get() == 5.
 * This test asserts assertEquals(7, x.get()) — claims x.get() == 7.
 *
 * The kit emits TWO operands in the contract's `and`:
 *   operand[0] = construction fact: =(get(x), 5)   [from Box.java ctor]
 *   operand[1] = test's claim:      =(get(x), 7)   [from this assertion]
 *
 * The solver sees =(t,5) ∧ =(t,7) — t cannot be both 5 and 7 → UNSATISFIED.
 *
 * NOTE: this is a SINGLE assertion with NO internal contradiction. Without the
 * construction operand it would wrongly discharge (opaque term equals anything).
 * The refutation comes solely from Box's constructor pinning x.get()==5.
 */
public class BoxUniverseTest {

    @Test
    public void testBoxGetWrongValue() {
        Box x = new Box(5);
        assertEquals(7, x.get());
    }
}
