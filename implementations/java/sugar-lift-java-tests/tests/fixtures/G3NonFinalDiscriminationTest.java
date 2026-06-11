// G3 fixture: DISCRIMINATION — field is NOT final; getter reassignable.
// The ctor assigns this.val = v, but val is mutable (no final modifier).
// Expected: construction NOT pinned (refusal diagnostic mentioning the field),
// contract has ONLY ONE operand (opaque term stays unconstrained).
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class G3NonFinalDiscriminationTest {

    // Receiver class with a non-final field — pin must be refused.
    static class G3MutableBox {
        private int val;
        G3MutableBox(int v) { this.val = v; }
        void set(int v) { this.val = v; }
        int get() { return this.val; }
    }

    @Test
    public void testMutableBoxGet() {
        G3MutableBox x = new G3MutableBox(5);
        assertEquals(5, x.get());
    }
}
