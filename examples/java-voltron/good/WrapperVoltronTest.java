import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * GOOD suite: two-layer construction chain, ctor pin and test claim agree.
 *
 * The receiver `w.unwrap()` is itself a method call — not a local variable.
 * The kit resolves it via mutual recursion:
 *   w → new Wrapper(new Box(5))
 *   unwrap() → this.box → Wrapper ctor param[0] → new Box(5)
 *   get() → this.value → Box ctor param[0] → 5
 *
 * Emits two operands in the contract's `and`:
 *   operand[0] = construction fact: =(get(w.unwrap__), 5)   [from Box.java ctor]
 *   operand[1] = test claim:        =(get(w.unwrap__), 5)   [from this assertion]
 *
 * Consistent: 5 == 5 → discharged.
 */
public class WrapperVoltronTest {

    @Test
    public void testTwoLayerChain() {
        Wrapper w = new Wrapper(new Box(5));
        assertEquals(5, w.unwrap().get());
    }
}
