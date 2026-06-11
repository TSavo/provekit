// Effectively-final fixture: field is `private int value` — NO final keyword.
// Assigned only in the constructor; getter is a pure field read.
// The fixedpoint scan proves single-ctor-assignment, closed membrane (private).
// Expected: G3 pin fires — contract has TWO operands.
public final class EFBox {
    private int value; // intentionally no `final` — effectively final by scan
    EFBox(int v) { this.value = v; }
    int get() { return this.value; }
}
