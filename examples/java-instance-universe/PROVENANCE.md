# PROVENANCE: java-instance-universe

## What is walked

The kit walks ctor-to-field-to-getter edges in the receiver class, deriving all facts exclusively from `com.sun.source.tree.*` nodes:

1. `NewClassTree` — the receiver construction expression (e.g. `new Box(5)`)
2. `ClassTree` — the receiver class, indexed from every `*.java` file in the workspace root
3. `MethodTree` (getter) — the method named at the call site; must be exactly one non-static match with the correct arity
4. `ReturnTree` — the getter body must be exactly one `return <expr>;` statement
5. `MemberSelectTree` / `IdentifierTree` — the return expression must be `this.field` or a bare field identifier
6. `VariableTree` — the field declaration; must carry `Modifier.FINAL`
7. `MethodTree` (constructor) — the constructor whose arity matches the construction argument count; body scanned for `this.field = <param>` (via `AssignmentTree`)
8. `NewClassTree.getArguments().get(paramIndex)` — the literal value passed at construction time; resolved via `asIntLiteral`

The resolved value is emitted as a second operand in the same location-keyed contract's `and`, making the solver see:

    =(call:get(x), ctorValue) [construction fact]  AND  =(call:get(x), testValue) [test claim]

Because both operands share the byte-identical `ctorJson` term, the solver unifies them. A correct test (testValue == ctorValue) is consistent and discharges. A wrong test (testValue != ctorValue) is unsatisfied — refuted by the class's own constructor.

## Honest scope (weak tier)

This is the minimal sound tier. The following cases are explicitly REFUSED by name (a named diagnostic is emitted; the opaque term stays unconstrained; no falsePass):

- Field is not declared `final`
- Field is assigned outside a constructor (mutation defeats the construction pin)
- Getter body has more than one statement, or the sole statement is not `return <expr>`
- Return expression is anything other than `this.field` or a bare field identifier (e.g. `return this.value + 1` is refused)
- Constructor argument at the resolved parameter index is not an int literal
- Multiple non-static methods match the name and arity (overload ambiguity)

A follow-up (not in scope here) would extend to: multi-param getters, String-sorted fields, fields initialized via `this.field = <literal>` directly (already handled), and chained delegation.
