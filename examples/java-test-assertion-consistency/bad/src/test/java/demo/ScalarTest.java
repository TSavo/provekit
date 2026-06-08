package demo;

import demo.assertions.LearnedAssertions;
import org.junit.jupiter.api.Test;

final class ScalarTest {
    @Test
    void scalarSumContradiction() {
        LearnedAssertions.assertSameValue(6, ScalarBox.scalarSum());
        LearnedAssertions.assertSameValue(7, ScalarBox.scalarSum());
    }
}
