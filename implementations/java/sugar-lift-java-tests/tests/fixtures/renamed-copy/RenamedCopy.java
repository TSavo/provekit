// Fixture (P4.5 test b): uses `check(1, g(2))` — a renamed copy of assertEquals.
// Must LIFT: the deriver classified `check` as EQUALITY from its throw-guard.
import org.junit.Test;
import static org.junit.custom.CheckAssert.check;

public class RenamedCopy {
    @Test
    public void testG() {
        check(1, g(2));
    }

    private int g(int x) { return x - 1; }
}
