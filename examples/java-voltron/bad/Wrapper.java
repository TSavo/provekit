// Voltron showcase BAD suite: identical to good/Wrapper.java.
// The two-layer chain resolves to pin value=5. The bad test claims 6.
final class Wrapper {
    private final Box box;
    Wrapper(Box b) { this.box = b; }
    Box unwrap() { return this.box; }
}
