// Voltron fixture: DISCRIMINATION — inner class (VoltronBoxMutable) has a non-final field.
// The whole two-layer chain must refuse — soundness teeth apply at every layer.
// Expected: assertion refused entirely (no contract emitted for this test method), or
// if a contract is emitted it has exactly ONE operand (no construction pin).
// A named diagnostic citing "not final" must be present.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class VoltronNonFinalDiscriminationTest {

    // Inner class whose field is NOT final — Voltron must refuse the whole chain.
    static final class VoltronBoxMutable {
        int value; // not final — pin is unsafe
        VoltronBoxMutable(int v) { this.value = v; }
        int get() { return this.value; }
    }

    static final class VoltronWrapperMutable {
        private final VoltronBoxMutable box;
        VoltronWrapperMutable(VoltronBoxMutable b) { this.box = b; }
        VoltronBoxMutable unwrap() { return this.box; }
    }

    @Test
    public void testNonFinalInnerField() {
        VoltronWrapperMutable w = new VoltronWrapperMutable(new VoltronBoxMutable(5));
        // Non-final field in VoltronBoxMutable — the chain must refuse, not pin.
        assertEquals(5, w.unwrap().get());
    }
}
