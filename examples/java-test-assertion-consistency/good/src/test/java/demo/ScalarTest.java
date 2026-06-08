package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

final class ScalarTest {
    @Test
    void scalarSumIsSix() {
        assertEquals(6, ScalarBox.scalarSum());
        assertTrue(ScalarBox.scalarSum() == 6);
    }
}
