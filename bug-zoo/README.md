# Bug Zoo

Bug Zoo is where ProvekIt proves the bug story against executable software.

A normal bug corpus says: this project failed, and this patch fixed it. Bug Zoo
asks for a stronger receipt: can ProvekIt rediscover the missing obligation
from the code, framework metadata, schemas, or annotations already present;
name that obligation by CID; and confirm the same specimen runs clean after the
source closes the boundary?

Each species is a small, realistic specimen that turns a bug-class claim into a
receipt. The key split is:

1. the host language's own compiler/kit maps source to a witnessed bug output;
   and
2. the self-contained Bug Zoo runner verifies that the resulting canonical
   ProofIR has the expected bytes and matches any required cross-language
   equivalences.

The normal project gate is `provekit prove`. Bug Zoo is machinery under
`bug-zoo/`, not a public `provekit` subcommand.

In shorthand, each language proves `k_lang(I) = t`: the language compiler
`k_lang` (as a ProvekIt kit/lifter) maps source `I` to witnessed output `t`.
Here `t` is not just an abstract shape; it is canonical ProofIR bytes, CID, and
receipt.

The zoo is not a patch archive. Historical fixes are context. The durable
artifact is independent rediscovery: ProvekIt found the missing `p => q` edge,
proved which source surfaces express it, and then verified that the fixed source
closes the edge under the same surface.

## Why It Matters

Bug Zoo makes the big claim falsifiable.

- Lifters are tested against real bug shapes: did the lift preserve the boundary that mattered?
- Fixes are tested against closure, not plausibility: did the fixed host artifact re-lift to the required ProofIR without the missing edge?
- Cross-domain correctness is tested directly: can different source surfaces collapse to the same claim boundary?
- The corpus prioritizes the substrate: recurring missing edges deserve first-class predicates and stronger kit coverage.

## Specimen States

Each species can carry four states:

- `lab/`: normal code or metadata that passes its ordinary host checks.
- `exhibit/`: the same bug species with one or more native contract surfaces; ProvekIt lifts the surface and reports the missing edge.
- `fixed/`: the paired exhibit source with the boundary closed; ProvekIt lifts the same surface and reports clean.
- `wild/`: real OSS specimens pinned by advisory, commit, affected path, and evidence.

## Receipt Stack

The checked-in null-boundary specimen tells a Green/Red/Green story:

```text
lab green -> exhibit red with missing edge -> fixed green with clean diagnostics
```

ProofIR is allowed to be lossy here. Specimens compare contract boundaries, not
host-language implementation detail.

## Current Species

| Species | Languages / surfaces | Missing edge | Shared ProofIR CID |
|---|---|---|---|
| `BZ-SHAPE-005` | Java: ProvekIt native `@NotNull`, Spring Web; TypeScript: zod, class-validator; C#: DataAnnotations, `//provekit:` annotations, LINQ | `maybe_null(name) => non_null(name)` | `blake3-512:0d611d8478a205ff040e7d0bcf6c21b12051340ecc5f00c3953af632b23fc01e069b4ad8a8699869163e135b9fde85792eba6acc54cd75cb3d3cc6a40a99ded4` |

## Run It

Run all species with the Bug Zoo runner:

```sh
cargo run --manifest-path bug-zoo/Cargo.toml -- --all
```

Run discovery directly for the current TypeScript and C# null-boundary examples:

```sh
pnpm exec tsx bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/tools/ts-boundary-discover.ts zod bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/exhibit/zod/harness

dotnet run --project implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj -- discover csharp-linq bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/exhibit/linq-where/harness
```

The discovery commands prove the language compiler/kit mapped source to a
witnessed output. The `provekit-bug-zoo` runner proves that witnessed output is
byte-identical for the specimen.
