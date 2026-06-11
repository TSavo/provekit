# PROVENANCE: java-voltron

Voltron (Mutually-Recursive Construction-Semantics Resolver)

## What is walked

The kit resolves the two-layer chain `w.unwrap().get()` from source, crossing two class boundaries.
All facts come exclusively from `com.sun.source.tree.*` nodes — no regex, no string scanning,
no hardcoded names.

### Layer 1: outer receiver `w.unwrap()`

The receiver of `.get()` is a `MethodInvocationTree` (not a local variable).
`resolveConstruction(w.unwrap(), 0, ssaBindings)` is called:

1. `MethodInvocationTree` with zero-arg method `unwrap`, receiver `w`
2. `w` is an `IdentifierTree` in `ssaBindings` (effectively-final local)
3. `ssaBindings["w"]` = `new Wrapper(new Box(5))` — a `NewClassTree`
4. `resolveConstruction(new Wrapper(new Box(5)), 1)` returns `ResolvedCtor("Wrapper", [new Box(5)])`
5. `ClassTree("Wrapper")` looked up in workspace index
6. `MethodTree("unwrap")` — exactly one non-static match, arity 0
7. `ReturnTree` — body is exactly one statement: `return this.box`
8. `MemberSelectTree` — return expression is `this.box` — field name `box`
9. `VariableTree("box")` — field declaration carries `Modifier.FINAL`
10. `isFieldMutatedOutsideCtor` — confirmed not mutated outside constructor
11. `MethodTree(ctor Wrapper(Box b))` — arity 1, matched by `rc.ctorArgs.size() == 1`
12. `paramIndexAssignedToField(ctor, "box")` — param index 0 (`this.box = b`)
13. `rc.ctorArgs.get(0)` = `new Box(5)` — recurse

### Layer 2: inner construction `new Box(5)`

`resolveConstruction(new Box(5), 2)` returns `ResolvedCtor("Box", [5])`.

Then `resolveIntFromChain` applies the outer method `get` to this construction:

14. `ClassTree("Box")` looked up in workspace index
15. `MethodTree("get")` — exactly one non-static match, arity 0
16. `ReturnTree` — body is exactly one statement: `return this.value`
17. `MemberSelectTree` — return expression is `this.value` — field name `value`
18. `VariableTree("value")` — field declaration carries `Modifier.FINAL`
19. `isFieldMutatedOutsideCtor` — confirmed not mutated outside constructor
20. `MethodTree(ctor Box(int v))` — arity 1, matched by `rc.ctorArgs.size() == 1`
21. `paramIndexAssignedToField(ctor, "value")` — param index 0 (`this.value = v`)
22. `rc.ctorArgs.get(0)` = `5` — `asIntLiteral(5)` = 5

The resolved value 5 is emitted as a second operand in the same location-keyed contract's `and`:

    =(call:get(w.unwrap__), 5)   [construction fact — two-layer walk from source]
    AND
    =(call:get(w.unwrap__), 5)   [test claim — assertEquals(5, ...)]

Both operands share the byte-identical receiver term `w.unwrap__` (derived from the receiver
expression text, used as the contract location label only). The solver unifies them.

A correct test (claim == pin) is consistent and discharges. A wrong test (claim != pin) is
unsatisfied — refuted by Box's own constructor, crossed via Wrapper.

## Honest scope (weak tier)

Only pure single-return-of-final-field getters are supported at every layer.
Any impurity anywhere in the chain causes the WHOLE chain to refuse with a named diagnostic:

- field not declared `final` at any layer (mutation defeats the pin)
- field assigned outside a constructor at any layer
- getter body has more than one statement, or is not `return <expr>` at any layer
- return expression is computation (e.g. `return this.value + 1`) at any layer
- constructor argument at the leaf is not an int literal
- chain depth exceeds 8 hops

Refusal keeps the assertion opaque and unconstrained (existing P5c behaviour) — never a falsePass.

## The asymmetry that matters

The BAD test has a SINGLE assertion `assertEquals(6, w.unwrap().get())` with NO internal
contradiction. Without the two-layer construction operand it would wrongly discharge — the
opaque term `call:get(w.unwrap__)` can equal anything.

With the construction pin:
    =(call:get(w.unwrap__), 5)   [two-layer ctor walk]
    AND
    =(call:get(w.unwrap__), 6)   [test claim]
→ UNSATISFIED. The refutation comes solely from Box's constructor, crossed via Wrapper's field.
