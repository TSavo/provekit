# BZ-SHAPE-005: Null Boundary Equivalence

This species captures a null-boundary shape bug: a boundary admits `maybe_null(name)`, while the downstream sink requires `non_null(name)`. The missing edge is:

```text
maybe_null(name) => non_null(name)
```

Java, TypeScript, and C# are exhibits of the same species. Each language shows a different surface syntax for the same semantic boundary condition, and each lifted exhibit preserves the same ProofIR predicate: `neq(name, null)`.

## Exhibits

- `java/`: `provekit-native`, `spring-web`
- `typescript/`: `zod`, `class-validator`
- `csharp/`: `data-annotations`, `provekit-annotations`, `linq-where`

## Evidence States

- `lab/` is green: ordinary native code runs, but there is no ProvekIt workflow.
- `exhibit/` is red: adding any supported contract surface exposes the missing edge to ProvekIt, and `provekit prove --formula` rejects the lab null witness against the lifted non-null requirement.
- `fixed/` is green: the same contract surface remains, the code is repaired, and `provekit prove --formula` discharges the paired non-null implication.

The checked receipt is both byte-level and proof-level: all surfaces lift to the same boundary CID, and every exhibit/fixed pair carries the expected red/green proof signal.

`wild/` is intentionally absent until a real sighting is pinned.
