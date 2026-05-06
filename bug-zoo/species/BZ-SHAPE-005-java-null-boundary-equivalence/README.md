# BZ-SHAPE-005: Java Null Boundary Equivalence

This specimen shows a null-boundary bug species.

The lab library compiles and runs with ordinary Java checks. The exposed variants add contract surfaces that engineers already use:

- `provekit-native`: explicit ProvekIt-style `@Requires("name != null")`
- `spring-web`: Spring's default required `@RequestParam`

The source surfaces are not equivalent as Java programs. Their contract boundary is equivalent. Both lift to the same ProofIR precondition: `neq(name, null)`.

The exposed bug is the missing edge from a caller that may provide null to a sink requiring non-null:

```text
maybe_null(name) => non_null(name)
```
