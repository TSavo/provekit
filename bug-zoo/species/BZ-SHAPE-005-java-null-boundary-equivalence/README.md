# BZ-SHAPE-005: Java Null Boundary Equivalence

This specimen is the small, sharp version of a bug almost every Java service has shipped: a value that may be null reaches a boundary that requires non-null.

The point is not that ProvekIt can recognize one annotation. The point is that different source surfaces can express the same boundary fact, and the zoo can prove they collapse to the same content-addressed claim.

The lab library compiles and runs with ordinary Java checks. Nothing about the host toolchain says the edge is missing. The exposed variants add contract surfaces that engineers already use:

- `provekit-native`: explicit ProvekIt-style `@Requires("name != null")`
- `spring-web`: Spring's default required `@RequestParam`

The source surfaces are not equivalent as Java programs. Their contract boundary is equivalent. Both lift to the same ProofIR precondition: `neq(name, null)`.

The exposed bug is the missing edge from a caller that may provide null to a sink requiring non-null:

```text
maybe_null(name) => non_null(name)
```

The dropped variant is not the historical fix. It is the Java ORP realizer emitting a native edge-closing shape, then being accepted only after the Java lifter sees the resulting `neq(name, null)` boundary.

The dropped variant now carries two realization receipts before the fix receipt:

- `proof-plan.json`: the target-neutral proof-first statement. It names the forbidden region `maybe_null(name) && !non_null(name)` and the eliminator that makes that region uninhabitable.
- `language-dropper.json`: the Java language-dropper projection. It says how that proof plan becomes the `@Requires("name != null")` source shape in this Java specimen.

The durable artifact is the fix receipt: it binds the generated host-language change to the exact missing edge it closed, the post-lift ProofIR, and the policy that admitted the closure.
