// Effectively-final Voltron positive: two-layer chain with no final keywords.
// Both EFWrapper.box and EFBox.value are private without final; scan proves both.
// Expected: contract has TWO operands (ctor fact + test claim), both const(9,Int).
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class EFVoltronPositiveTest {

    @Test
    public void testEFTwoLayerChain() {
        EFWrapper w = new EFWrapper(new EFBox(9));
        assertEquals(9, w.unwrap().get());
    }
}
