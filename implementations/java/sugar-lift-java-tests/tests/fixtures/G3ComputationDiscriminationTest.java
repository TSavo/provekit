// G3 fixture: DISCRIMINATION — getter body is `return this.value + 1` (not a pure field read).
// Step 4 of resolveIntResult must refuse this — the return expression is a binary
// addition, not a `this.field` or bare identifier.
// Expected: contract has ONLY ONE operand (construction not pinned; refused at step 4).
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class G3ComputationDiscriminationTest {

    // Getter does arithmetic — not a pure field read.
    static class G3ComputeBox {
        private final int value;
        G3ComputeBox(int v) { this.value = v; }
        int get() { return this.value + 1; }
    }

    @Test
    public void testComputeBoxGet() {
        G3ComputeBox x = new G3ComputeBox(5);
        // Whatever the test claims — construction is not pinned (getter is not pure).
        assertEquals(6, x.get());
    }
}
