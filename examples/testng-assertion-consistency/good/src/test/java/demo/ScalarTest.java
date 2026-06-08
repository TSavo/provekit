package demo;

import static org.testng.Assert.assertEquals;
import static org.testng.Assert.assertTrue;

import org.testng.annotations.Test;

public final class ScalarTest {
    @Test
    public void scalarSumIsSix() {
        assertEquals(ScalarBox.scalarSum(), 6);
        assertTrue(ScalarBox.scalarSum() == 6);
    }
}

