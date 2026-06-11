import org.junit.Test;
import static org.junit.Assert.assertEquals;

/**
 * GOOD suite: construction fact and test claim agree.
 * new Box(5).get() == 5 by construction; test asserts 5.
 * Mint conjoins: =(get(x),5) [ctor] ∧ =(get(x),5) [test] → consistent → discharged.
 */
public class BoxUniverseTest {

    @Test
    public void testBoxGetValue() {
        Box x = new Box(5);
        assertEquals(5, x.get());
    }
}
