package demo;

import javax.validation.constraints.Max;
import javax.validation.constraints.Min;
import javax.validation.constraints.NotNull;
import javax.validation.constraints.Size;

public final class Chain {
    private Chain() {}

    @Max(10)
    public static int maxProducer() {
        return 6;
    }

    public static int maxConsumer(@Max(10) int value) {
        return value;
    }

    public static int maxEdge() {
        return maxConsumer(maxProducer());
    }

    @Size(min = 1, max = 5)
    public static String sizeProducer() {
        return "abc";
    }

    public static String sizeConsumer(@Size(min = 1, max = 5) String value) {
        return value;
    }

    public static String sizeEdge() {
        return sizeConsumer(sizeProducer());
    }

    @NotNull
    public static String notNullProducer() {
        return "value";
    }

    public static String notNullConsumer(@NotNull String value) {
        return value;
    }

    public static String notNullEdge() {
        return notNullConsumer(notNullProducer());
    }

    @Min(0)
    @Max(10)
    public static int rangeProducer() {
        return 6;
    }

    public static int rangeConsumer(@Min(0) @Max(10) int value) {
        return value;
    }

    public static int rangeEdge() {
        return rangeConsumer(rangeProducer());
    }
}

