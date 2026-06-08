package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

final class ScalarTest {
    @Test
    void scalarSumContradiction() {
        assertEquals(6, ScalarBox.scalarSum());
        assertEquals(7, ScalarBox.scalarSum());
    }
}
