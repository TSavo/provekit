package zoo;

public final class AmountFlowHarness {
    public static void main(String[] args) {
        int value = AmountParser.parseInt("42");
        if (value != 42) {
            throw new AssertionError("expected documented sample to parse as 42");
        }
    }
}
