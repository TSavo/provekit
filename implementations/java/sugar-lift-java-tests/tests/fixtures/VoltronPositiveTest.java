// Voltron fixture: POSITIVE — two-layer construction chain discharges.
// w.unwrap() is a MethodInvocationTree receiver; resolveConstruction walks:
//   w → new VoltronWrapper(new VoltronBox(5)) → unwrap() → this.box → VoltronBox ctor arg[0]
//   → new VoltronBox(5) → get() → this.value → VoltronBox ctor arg[0] → 5
// Expected: contract has TWO operands — operand[0] = ctor pin (5), operand[1] = test claim (5).
// Both use the same receiver term; both are const(5,Int).
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class VoltronPositiveTest {

    @Test
    public void testTwoLayerChain() {
        VoltronWrapper w = new VoltronWrapper(new VoltronBox(5));
        assertEquals(5, w.unwrap().get());
    }
}
