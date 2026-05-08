package zoo;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

public final class AmountParserTest {
    @Test
    void parsesRequiredSample() {
        int value = AmountParser.parseInt("43");
        assertEquals(43, value);
    }
}
