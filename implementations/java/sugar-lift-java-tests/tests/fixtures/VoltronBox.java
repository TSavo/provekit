// Voltron fixture: inner class for two-layer construction chain.
// Box holds a final int value; pure getter returns it.
public final class VoltronBox {
    private final int value;
    VoltronBox(int v) { this.value = v; }
    int get() { return this.value; }
}
