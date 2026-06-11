import org.junit.Test;
import static org.junit.Assert.assertEquals;
public class RecurrenceLift {
    @Test public void testDrives() { assertEquals(1, ident(1)); }
    private int ident(int x) { return x; }
}
