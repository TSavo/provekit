package demo;

import javax.validation.constraints.Max;
import javax.validation.constraints.Min;
import javax.validation.constraints.NotNull;
import javax.validation.constraints.Size;

public final class Chain {
    private Chain() {}

    @Max(100)
    public static int maxProducer() {
        return 50;
    }

    public static int maxConsumer(@Max(10) int value) {
        return value;
    }

    public static int maxEdge() {
        return maxConsumer(maxProducer());
    }

    @Size(min = 0, max = 100)
    public static String sizeProducer() {
        return "too-long-value";
    }

    public static String sizeConsumer(@Size(min = 0, max = 10) String value) {
        return value;
    }

    public static String sizeEdge() {
        return sizeConsumer(sizeProducer());
    }

    public static String nullableProducer() {
        return null;
    }

    public static String notNullConsumer(@NotNull String value) {
        return value;
    }

    public static String notNullEdge() {
        return notNullConsumer(nullableProducer());
    }

    @Min(0)
    @Max(100)
    public static int rangeProducer() {
        return 5;
    }

    public static int rangeConsumer(@Min(10) @Max(90) int value) {
        return value;
    }

    public static int rangeEdge() {
        return rangeConsumer(rangeProducer());
    }
}

