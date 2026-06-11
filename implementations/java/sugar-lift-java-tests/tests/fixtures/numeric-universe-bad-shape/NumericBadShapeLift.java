// Fixture: numeric-universe-walk with an unsupported vendor body shape.
// The equality contract still lifts; the universe row must NOT (named refusal).
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class NumericBadShapeLift {

    @Test
    public void testSquareTwo() {
        // square(2) == 4 — equality lifts; universe row must be refused by name
        assertEquals(4, square(2));
    }
}
