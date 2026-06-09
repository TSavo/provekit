package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

public class ChainTest {
    @Test
    void bodyguardPreconditionSatisfiedAtCallsite() {
        assertEquals(16, Chain.edge());
    }
}
