// Voltron showcase: inner class — holds a final int, returned by a pure getter.
// The kit walks: ClassTree(Box) → MethodTree(get) → ReturnTree → MemberSelectTree(this.value)
// → VariableTree(final value) → MethodTree(ctor Box(int)) → param assignment → int literal.
final class Box {
    private final int value;
    Box(int v) { this.value = v; }
    int get() { return this.value; }
}
