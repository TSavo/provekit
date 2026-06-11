// Effectively-final G3 positive: EFBox.value is private, no final keyword.
// The fixedpoint scan proves single-ctor-assignment; G3 pin must fire.
// Expected: contract has TWO operands (ctor fact + test claim), both const(7,Int).
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class EFBoxPositiveTest {

    @Test
    public void testEFBoxGet() {
        EFBox x = new EFBox(7);
        assertEquals(7, x.get());
    }
}
