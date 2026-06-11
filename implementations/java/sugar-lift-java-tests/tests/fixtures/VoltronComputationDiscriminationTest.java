// Voltron fixture: DISCRIMINATION — inner Box getter is computed (return this.value + 1).
// resolveConstruction walks through Wrapper.unwrap() fine, but when it tries to resolve
// the inner Box construction further (via get()), extractFieldName on `this.value + 1`
// returns null → chain refuses entirely.
// Expected: assertion refused (no contract, or 1-operand contract), named diagnostic present.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class VoltronComputationDiscriminationTest {

    static final class VoltronBoxComputed {
        private final int value;
        VoltronBoxComputed(int v) { this.value = v; }
        int get() { return this.value + 1; } // computation — not a pure field read
    }

    static final class VoltronWrapperComputed {
        private final VoltronBoxComputed box;
        VoltronWrapperComputed(VoltronBoxComputed b) { this.box = b; }
        VoltronBoxComputed unwrap() { return this.box; }
    }

    @Test
    public void testComputedInnerGetter() {
        VoltronWrapperComputed w = new VoltronWrapperComputed(new VoltronBoxComputed(5));
        // get() does arithmetic — chain must refuse at the inner getter step.
        assertEquals(6, w.unwrap().get());
    }
}
