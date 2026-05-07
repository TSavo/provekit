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

- `lab/` is green: ordinary native code runs, but the bug class is not exposed to ProveKit.
- `exhibit/` is red: adding any supported contract surface exposes the missing edge to ProveKit.
- `fixed/` is green: the same contract surface remains, the code is repaired, and ProveKit runs clean.

Fix receipts are not checked into this specimen. They can be derived later from an `exhibit/` and `fixed/` pair.

`wild/` is intentionally absent until a real sighting is pinned.
