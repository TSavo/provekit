package demo;

import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;

public class ChainTest {
    @Test
    void bodyguardPreconditionViolationThrowsAtRuntime() {
        assertThrows(IllegalArgumentException.class, Chain::edge);
    }
}
