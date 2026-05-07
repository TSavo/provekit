# Bug Zoo: Executable Bug Species

Bug Zoo is ProvekIt's executable evidence that bug classes have portable
semantic shape.

The claim is not that TypeScript, Java, and C# fail in the same way at runtime.
They do not. A TypeScript `TypeError`, a Java `NullPointerException`, and a C#
`NullReferenceException` are different host-language events. The claim is that
after each language uses its own kit to lift the bug, the missing obligation can
collapse to the same canonical ProofIR bytes.

For the null-boundary species, that shared shape is:

```text
maybe_null(name) => non_null(name)
```

and the canonical ProofIR boundary is:

```text
neq(name, null)
```

## The Two Steps

Each zoo specimen separates discovery from verification.

1. **Language discovery.** The host language uses its own toolchain and kit.
   Java runs Java lifters through Maven and the JVM. TypeScript runs the
   TypeScript lifter (`liftPath`) through `pnpm exec tsx`. C# runs the C#
   lifters through `dotnet`. The specimen does not reimplement the kit inside
   the zoo; it asks the kit to find the bug in native source.
2. **Proof verification.** The normal project gate is `provekit prove`. Bug Zoo
   owns a self-contained runner under `bug-zoo/`: it receives canonical Bug Zoo
   ProofIR from the lifter RPC, hashes it, compares it to checked-in witness
   bytes, and checks required equivalences across surfaces and languages.

In shorthand: each language proves `k_lang(I) = t`, where `k_lang` is the
language compiler as a ProvekIt kit/lifter, `I` is source, and `t` is witnessed
output: canonical ProofIR bytes, CID, and receipt. When TypeScript, Java, and
C# all land on the same `t`, the bug has a portable signature independent of
its host-language syntax or exception type.

## Current Null-Boundary Receipts

The current zoo includes:

- `BZ-SHAPE-005`: Java null boundary through ProvekIt-native annotations and
  Spring Web `@RequestParam`.
- `BZ-SHAPE-006`: TypeScript null boundary through zod and class-validator.
- `BZ-SHAPE-007`: C# null boundary through DataAnnotations,
  `//provekit:` annotations, and LINQ `Where(name => name != null)`.

All three species expose the same missing edge:

```text
maybe_null(name) => non_null(name)
```

and the same ProofIR CID:

```text
blake3-512:0d611d8478a205ff040e7d0bcf6c21b12051340ecc5f00c3953af632b23fc01e069b4ad8a8699869163e135b9fde85792eba6acc54cd75cb3d3cc6a40a99ded4
```

That CID is the receipt. The source languages disagree; the witnessed output
does not.

## Run It

From the repository root:

```sh
cargo run --manifest-path bug-zoo/Cargo.toml -- --all
```

You can also run each discovery step directly:

```sh
pnpm exec tsx bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/tools/ts-boundary-discover.ts zod bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence/exposed/zod/harness

dotnet run --project implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj -- discover csharp-linq bug-zoo/species/BZ-SHAPE-007-csharp-null-boundary-equivalence/exposed/linq-where/harness
```

Those commands show the first phase: the language compiler/kit maps source to a
witnessed bug output. The `provekit-bug-zoo` runner is the lab harness for the
second phase: proving the witnessed output is byte-identical for the specimen.

## Why This Matters

Bug Zoo turns the broad ProvekIt thesis into receipts:

- ordinary code passes ordinary host checks;
- each language's own compiler/kit maps source to a witnessed missing edge;
- canonical ProofIR makes equivalent bug shapes hash to the same bytes;
- droppers can close the edge only if re-lift verifies the closure.

It is not a patch archive and not a benchmark of historical remediations. It is
a laboratory for the substrate claim: bug classes are tractable to universal
semantic shapes once lifted below language syntax.
