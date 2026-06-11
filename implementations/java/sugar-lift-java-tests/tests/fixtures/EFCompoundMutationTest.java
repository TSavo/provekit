// Effectively-final discrimination: private field with `value++` in a method.
// The old stmtAssignsField only checked AssignmentTree — compound operators were invisible.
// Expected: 1 contract, 1 operand (no ctor pin), diagnostic names outside-constructor assignment.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class EFCompoundMutationTest {

    // Private field with post-increment in a method — not effectively final.
    static final class IncrementBox {
        private int value;
        IncrementBox(int v) { this.value = v; }
        void increment() { this.value++; } // UnaryTree — old code missed this entirely
        int get() { return this.value; }
    }

    @Test
    public void testIncrementBox() {
        IncrementBox x = new IncrementBox(5);
        assertEquals(5, x.get());
    }
}
