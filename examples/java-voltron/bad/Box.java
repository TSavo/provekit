// Voltron showcase BAD suite: identical to good/Box.java.
// The construction pins value=5. The bad test claims 6 — contradiction.
final class Box {
    private final int value;
    Box(int v) { this.value = v; }
    int get() { return this.value; }
}
