// Voltron fixture: outer class for two-layer construction chain.
// Wrapper holds a final VoltronBox; unwrap() returns this.box.
public final class VoltronWrapper {
    private final VoltronBox box;
    VoltronWrapper(VoltronBox b) { this.box = b; }
    VoltronBox unwrap() { return this.box; }
}
