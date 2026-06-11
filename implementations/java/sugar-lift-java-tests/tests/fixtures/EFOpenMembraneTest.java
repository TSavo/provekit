// Effectively-final discrimination: field is package-private (no modifier).
// Assignment universe escapes the walked class — cannot establish effective finality.
// Expected: 1 contract, 1 operand (no ctor pin), diagnostic names open membrane.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class EFOpenMembraneTest {

    // Package-private field — open membrane; Voltron must refuse the pin.
    static final class PackagePrivateBox {
        int value; // no modifier — assignment universe is open
        PackagePrivateBox(int v) { this.value = v; }
        int get() { return this.value; }
    }

    @Test
    public void testOpenMembrane() {
        PackagePrivateBox x = new PackagePrivateBox(3);
        assertEquals(3, x.get());
    }
}
