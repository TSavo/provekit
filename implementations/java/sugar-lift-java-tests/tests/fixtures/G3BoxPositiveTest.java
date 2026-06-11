// G3 fixture: POSITIVE case — pure final-field getter, construction pins the value.
// Box.value is final; ctor assigns this.value = v; get() returns this.value.
// new Box(5).get() == 5 BY CONSTRUCTION.
// Expected: contract has TWO operands in inv.and — operand[0] is the ctor fact (5),
// operand[1] is the test's claim (5). Both refer to the same ctorJson term.
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class G3BoxPositiveTest {

    @Test
    public void testBoxGet() {
        G3Box x = new G3Box(5);
        assertEquals(5, x.get());
    }
}
