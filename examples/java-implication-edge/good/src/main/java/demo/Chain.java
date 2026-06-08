package demo;

import javax.validation.constraints.Min;

public final class Chain {
    private Chain() {}

    @Min(0)
    public static int producer() {
        return 6;
    }

    public static int consumer(@Min(0) int value) {
        if (value < 0) {
            throw new IllegalArgumentException("negative");
        }
        return value;
    }

    public static int edge() {
        return consumer(producer());
    }
}
