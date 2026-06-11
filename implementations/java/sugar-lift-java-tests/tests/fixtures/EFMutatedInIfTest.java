// Effectively-final discrimination: private field assigned inside a method's for-loop body.
// The OLD hand-rolled stmtAssignsField had no ForLoopTree case — it would silently MISS
// this assignment and incorrectly accept the pin. The TreeScanner must catch it.
// Expected: 1 contract, 1 operand (no ctor pin), diagnostic names outside-constructor assignment.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class EFMutatedInIfTest {

    // Private field assigned inside a for-loop body in a method.
    static final class MutatedInForBox {
        private int value;
        MutatedInForBox(int v) { this.value = v; }
        // Old stmtAssignsField had no ForLoopTree/WhileLoopTree case — would miss this.
        void reset() {
            for (int i = 0; i < 1; i++) {
                this.value = 0; // assignment inside for-body — TreeScanner finds it
            }
        }
        int get() { return this.value; }
    }

    @Test
    public void testMutatedInFor() {
        MutatedInForBox x = new MutatedInForBox(4);
        assertEquals(4, x.get());
    }
}
