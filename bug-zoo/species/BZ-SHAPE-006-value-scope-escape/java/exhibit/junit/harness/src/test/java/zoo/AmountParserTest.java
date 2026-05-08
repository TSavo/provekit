package zoo;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

public final class AmountParserTest {
    @Test
    void parsesDocumentedSample() {
        int value = AmountParser.parseInt("42");
        assertEquals(42, value);
    }
}
