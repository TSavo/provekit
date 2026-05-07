# BZ-SHAPE-006: TypeScript Null Boundary Equivalence

This specimen shows the TypeScript face of the same null-boundary bug species as the Java null-boundary specimen.

The lab library compiles and runs with ordinary TypeScript tooling. The exposed variants add contract surfaces TypeScript teams already use:

- `zod`: `z.object({ name: z.string() })`
- `class-validator`: `@IsString() name: string`

The source surfaces are not equivalent as TypeScript programs. Their contract boundary is equivalent. The specimen-owned TS lift RPC uses the TypeScript kit to recognize each surface, then emits the same Bug Zoo ProofIR contract boundary:

```text
neq(name, null)
```

The exposed bug is the missing edge from a caller that may provide a nullish value to a sink requiring non-null:

```text
maybe_null(name) => non_null(name)
```

The important receipt is byte identity at the ProofIR boundary. A TypeScript nullish boundary and a Java null boundary should collapse to the same canonical bytes when they describe the same missing obligation.

The dropped variant is a TypeScript-native transform. The realizer inserts a nullish guard before `toUpperCase()`, then returns a post-lift closure document. The zoo accepts it only when the generated source matches the checked-in dropped artifact, the closure ProofIR hashes to the same null-boundary IR, and the proof plan, language-dropper projection, transformed source, post-lift document, and closure witness all bind into the fix receipt.
