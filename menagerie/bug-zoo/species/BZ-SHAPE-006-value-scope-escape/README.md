# BZ-SHAPE-006: Value Scope Escape

This species catches a point witness escaping its value scope and being treated
as though it satisfied a stronger callsite requirement.

The `lab/` state is ordinary Java only: it runs the host harness and has no
ProvekIt workflow.

The Java specimen carries two exhibit surfaces:

- `junit/`: a JUnit assertion witnesses `parseInt("42") == 42`.
- `spring/`: a Spring request parameter default witnesses `value == 42` while
  Bean Validation requires `value >= 43`.

Both exhibits expose the same missing edge:

```text
eq(value, 42) => gte(value, 43)
```

The paired fixed surfaces change the witnessed value to 43. The requirement is
unchanged. Exhibit checks route through `provekit prove --formula` and produce
the red signal; fixed checks route through the same CLI path and produce the
green signal.
