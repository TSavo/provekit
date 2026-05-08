# Bug Zoo

Bug Zoo is where ProvekIt proves the bug story against executable software.

A normal bug corpus says: this project failed, and this patch fixed it. Bug Zoo
asks for a stronger receipt: can ProvekIt rediscover the missing obligation
from the code, framework metadata, schemas, or annotations already present;
name that obligation by CID; and confirm the same specimen runs clean after the
source closes the boundary?

Each species is a small, realistic specimen that turns a bug-class claim into a
receipt. The key split is:

1. `lab/` runs only the host language's ordinary check, with no ProvekIt
   workflow attached;
2. `exhibit/` and `fixed/` are ProvekIt projects: the Rust CLI invokes the
   native lifter and, when the specimen declares a composition edge, runs
   `provekit prove --formula`; and
3. the self-contained Bug Zoo runner verifies that the resulting canonical
   ProofIR has the expected bytes, matches any required cross-language
   equivalences, and routes scoped implication checks through
   `provekit prove --formula`.

The normal project gate is `provekit prove`. Bug Zoo is machinery under
`bug-zoo/`, not a public `provekit` subcommand; when a specimen needs a proof
answer, the runner invokes the Rust CLI.

In shorthand, each language proves `k_lang(I) = t`: the language compiler
`k_lang` (as a ProvekIt kit/lifter) maps source `I` to witnessed output `t`.
Here `t` is not just an abstract shape; it is canonical ProofIR bytes, CID, and
receipt.

Native evidence is projected into canonical truth; then the correctness layer
proves the missing edge or its closure.

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

- `lab/`: normal code or metadata that passes its ordinary host checks; it has
  no `.provekit` config and no lifter/prover workflow.
- `exhibit/`: the same bug species with one or more native contract surfaces; ProvekIt lifts the surface and reports the missing edge.
- `fixed/`: the paired exhibit source with the boundary closed; ProvekIt lifts the same surface and reports clean.
- `wild/`: real OSS specimens pinned by advisory, commit, affected path, and evidence.

## Organization

Bug Zoo is species-first. The runner lives at the root, but language behavior is
owned by each specimen. A shape id names the bug class; exhibits under the
specimen show the native surfaces that witness that same missing edge.

```text
bug-zoo/
  Cargo.toml
  src/
  species/
    BZ-SHAPE-005-null-boundary-equivalence/
      specimen.yaml
      java/
        lab/
        exhibit/{provekit-native,spring-web}/
        fixed/{provekit-native,spring-web}/
      typescript/
        lab/
        exhibit/{zod,class-validator}/
        fixed/{zod,class-validator}/
      csharp/
        lab/
        exhibit/{data-annotations,provekit-annotations,linq-where}/
        fixed/{data-annotations,provekit-annotations,linq-where}/
    BZ-SHAPE-006-value-scope-escape/
      specimen.yaml
      java/
        lab/
        exhibit/{junit,spring}/
        fixed/{junit,spring}/
```

`specimen.yaml` is the map. It declares the missing edge, the host-check command
for each language's lab, each exhibit/fixed harness project, the expected
ProofIR and diagnostic files, the paired fixed surface, and either equivalence
pairs that must hash to the same boundary CID or composition checks that must
reject or satisfy a scoped implication. Exhibit and fixed harnesses carry
`.provekit/config.toml` and `.provekit/lift/<surface>/manifest.toml`; the
runner invokes `provekit mint` so the CLI, not the zoo, drives the native RPC
lifter. Scoped implications are handed back to the CLI with
`provekit prove --formula`.

The repeated `lab/`, `exhibit/`, and `fixed/` code is intentional. Each language
gets to use its own native surface; the runner only asks whether those surfaces
lift to the same content-addressed boundary.

## Receipt Stack

The checked-in null-boundary specimen tells a Green/Red/Green story:

```text
lab host green -> exhibit prove red -> fixed prove green
```

ProofIR is allowed to be lossy here. Specimens compare contract boundaries, not
host-language implementation detail.

## Current Species

| Species | Languages / surfaces | Missing edge | Receipt |
|---|---|---|---|
| `BZ-SHAPE-005` | Java: ProvekIt native `@NotNull`, Spring Web; TypeScript: zod, class-validator; C#: DataAnnotations, `//provekit:` annotations, LINQ | `maybe_null(name) => non_null(name)` | shared boundary CID plus `provekit prove --formula` red/green composition checks |
| `BZ-SHAPE-006` | Java exhibits: JUnit point assertion; Spring `@RequestParam(defaultValue=...)` with Bean Validation | `eq(value, 42) => gte(value, 43)` | surface-specific witness CIDs; the checked property is the failed/satisfied composition edge |

## Run It

Run all species with the Bug Zoo runner:

```sh
cargo run --manifest-path bug-zoo/Cargo.toml -- --all
```

Trace a slow or hanging run:

```sh
PROVEKIT_BUG_ZOO_TRACE=1 cargo run --manifest-path bug-zoo/Cargo.toml -- --all --json
```

Trace output is written to stderr. The zoo logs host checks, `provekit mint`,
and `provekit prove --formula` boundaries with elapsed time. It also enables
`PROVEKIT_CLI_TRACE=1` for spawned CLI work, which prints mint RPC milestones
for `initialize`, `lift`, and `shutdown`.

Run discovery directly for the current TypeScript and C# null-boundary examples:

```sh
pnpm exec tsx bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/tools/ts-boundary-discover.ts zod bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/exhibit/zod/harness

dotnet run --project implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj -- discover csharp-linq bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/exhibit/linq-where/harness
```

The discovery commands prove the language compiler/kit mapped source to a
witnessed output. The `provekit-bug-zoo` runner proves that witnessed output is
byte-identical for the specimen, then delegates scoped implication verdicts to
`provekit prove --formula`.
