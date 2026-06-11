// G3 fixture: the receiver class with a pure final-field getter.
// Used by G3BoxPositiveTest, G3NonFinalDiscriminationTest.
// This version: field IS final — construction pins the value.
public final class G3Box {
    private final int value;
    G3Box(int v) { this.value = v; }
    int get() { return this.value; }
}
