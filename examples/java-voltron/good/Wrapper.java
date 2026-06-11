// Voltron showcase: outer class — holds a final Box, returned by a pure getter.
// The kit walks: ClassTree(Wrapper) → MethodTree(unwrap) → ReturnTree → MemberSelectTree(this.box)
// → VariableTree(final box) → MethodTree(ctor Wrapper(Box)) → param assignment → Box ctor arg.
// The Box ctor arg is `new Box(5)` — itself a NewClassTree — which the recursive resolver
// then walks through Box's own ctor→field→literal chain to pin the int value 5.
final class Wrapper {
    private final Box box;
    Wrapper(Box b) { this.box = b; }
    Box unwrap() { return this.box; }
}
