# BZ-SHAPE-006: TypeScript Null Boundary Equivalence

This specimen shows the TypeScript face of the same null-boundary bug species as the Java null-boundary specimen.

The lab library compiles and runs with ordinary TypeScript tooling. The exposed variants add contract surfaces TypeScript teams already use:

- `zod`: `z.object({ name: z.string() })`
- `class-validator`: `@IsString() name: string`

The source surfaces are not equivalent as TypeScript programs. Their contract
boundary is equivalent. The specimen-owned TS discovery CLI uses the TypeScript
kit to recognize each surface, then the lift RPC emits the same Bug Zoo ProofIR
contract boundary:

```text
neq(name, null)
```

The exposed bug is the missing edge from a caller that may provide a nullish value to a sink requiring non-null:

```text
maybe_null(name) => non_null(name)
```

The important receipt is byte identity at the ProofIR boundary. A TypeScript nullish boundary and a Java null boundary should collapse to the same canonical bytes when they describe the same missing obligation.

The dropped variant is a TypeScript-native transform. The realizer inserts a nullish guard before `toUpperCase()`, then returns a post-lift closure document. The Bug Zoo harness accepts it only when the generated source matches the checked-in dropped artifact, the closure ProofIR hashes to the same null-boundary IR, and the proof plan, language-dropper projection, transformed source, post-lift document, and closure witness all bind into the fix receipt.

The specimen has two explicit phases:

1. Run the TypeScript discovery CLI to discover the boundary from TypeScript
   source using the requested TypeScript adapter. For example, from the repo
   root:

   ```bash
   pnpm exec tsx bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/tools/ts-boundary-discover.ts zod bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/exposed/zod/harness
   ```

2. Run the Bug Zoo runner, which asks the lifter RPC for canonical ProofIR
   and checks the byte-identical CID against the other TypeScript exposure and
   the Java/C# null-boundary witnesses:

   ```bash
   cargo run --manifest-path bug-zoo/Cargo.toml -- bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence
   ```
