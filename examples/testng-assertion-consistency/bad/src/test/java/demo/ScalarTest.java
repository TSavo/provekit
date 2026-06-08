package demo;

import static org.testng.Assert.assertEquals;

import org.testng.annotations.Test;

public final class ScalarTest {
    @Test
    public void scalarSumContradiction() {
        assertEquals(ScalarBox.scalarSum(), 6);
        assertEquals(ScalarBox.scalarSum(), 7);
    }
}

