// Receiver class for the instance-universe showcase (BAD suite).
// Identical to the GOOD suite Box — the construction pins value=5.
// The bad test claims 7, contradicting the ctor-pinned fact 5.
final class Box {
    private final int value;
    Box(int v) { this.value = v; }
    int get() { return this.value; }
}
