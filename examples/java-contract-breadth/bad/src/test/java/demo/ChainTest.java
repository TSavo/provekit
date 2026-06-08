package demo;

import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

public class ChainTest {
    @Test
    void maxPostDoesNotEntailConsumerPre() {
        assertTrue(Chain.maxEdge() <= 10);
    }

    @Test
    void sizePostDoesNotEntailConsumerPre() {
        assertTrue(Chain.sizeEdge().length() <= 10);
    }

    @Test
    void missingNotNullPostDoesNotEntailConsumerPre() {
        assertNotNull(Chain.notNullEdge());
    }

    @Test
    void rangePostDoesNotEntailConsumerPre() {
        int value = Chain.rangeEdge();
        assertTrue(value >= 10 && value <= 90);
    }
}

