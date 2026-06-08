package demo;

import demo.assertions.LearnedAssertions;
import org.junit.jupiter.api.Test;

final class ScalarTest {
    @Test
    void scalarSumIsSix() {
        LearnedAssertions.assertSameValue(6, ScalarBox.scalarSum());
    }
}
