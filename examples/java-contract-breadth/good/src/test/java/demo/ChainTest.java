package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

public class ChainTest {
    @Test
    void broadenedContractsAllHold() {
        assertEquals(6, Chain.maxEdge());
        assertEquals("abc", Chain.sizeEdge());
        assertNotNull(Chain.notNullEdge());
        assertEquals(6, Chain.rangeEdge());
        assertTrue(Chain.sizeEdge().length() <= 5);
    }
}

