// Receiver class for the instance-universe showcase.
// The getter is a pure final-field read; ctor pins value via this.value = v.
// The kit walks: get() -> this.value (final) -> ctor param 0 -> new Box(5) -> 5.
final class Box {
    private final int value;
    Box(int v) { this.value = v; }
    int get() { return this.value; }
}
