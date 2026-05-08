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
3. the self-contained Bug Zoo runner verifies that the resulting projection has
   the expected addressable ProofIR shape CID or LinkBundle receipt CID, checks
   any required cross-language equivalences, and routes scoped implication
   checks through `provekit prove --formula`.

The normal project gate is `provekit prove`. Bug Zoo is machinery under
`menagerie/bug-zoo/`, not a public `provekit` subcommand; when a specimen needs a proof
answer, the runner invokes the Rust CLI.

In shorthand, each language proves `k_lang(I) = t`: the language compiler
`k_lang` (as a ProvekIt kit/lifter) maps source `I` to witnessed output `t`.
Here `t` is not just an abstract shape; it is an addressable canonical object.
For contract boundaries, the shape is named by a ProofIR CID. For cross-kit
calls, the bridge derivation is named by a LinkBundle receipt CID.

Each native surface maps through a structure-preserving homomorphism into the
correctness object; the proof layer checks whether the mapped obligation
commutes with equivalent surfaces or closes under the fixed witness.

The zoo is not a patch archive. Historical fixes are context. The durable
artifact is independent rediscovery: ProvekIt found the missing `p => q` edge,
proved which source surfaces express it, and then verified that the fixed source
closes the edge under the same surface.

## Why It Matters

Bug Zoo makes the big claim falsifiable.

- Lifters are tested against real bug shapes: did the lift preserve the boundary that mattered?
- Fixes are tested against closure, not plausibility: did the fixed host artifact re-lift or re-link to the required receipt without the missing edge?
- Cross-domain correctness is tested directly: can different source surfaces project to the same claim boundary?
- The corpus prioritizes the substrate: recurring missing edges deserve first-class predicates and stronger kit coverage.

## Specimen States

Each species can carry four states:

- `lab/`: normal code or metadata that passes its ordinary host checks; it has
  no `.provekit` config and no lifter/prover workflow.
- `exhibit/`: the same bug species with one or more native contract surfaces; ProvekIt lifts the surface and reports the missing edge.
- `fixed/`: the paired exhibit source with the boundary closed; ProvekIt lifts the same surface and reports clean.
- `wild/`: optional real OSS sightings pinned by advisory, commit, affected
  path, and evidence. The current runner reports `wildSightings` metadata but
  does not execute wild specimens yet.

## Organization

Bug Zoo is species-first. The runner lives at the destination root, but language
behavior is owned by each specimen. A shape id names the bug class; exhibits
under the specimen show the native surfaces that witness that same missing edge.

```text
menagerie/bug-zoo/
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
    BZ-SHAPE-007-polyglot-link-obligation/
      specimen.yaml
      rust-go/
        lab/
        exhibit/cgo-rust-callee/
        fixed/cgo-rust-callee/
```

`specimen.yaml` is the map. It declares the missing edge, the host-check command
for each language's lab, each exhibit/fixed harness project, the expected
ProofIR, link bundle, and diagnostic files, the paired fixed surface, and either
equivalence pairs that must hash to the same boundary CID or composition checks
that must reject or satisfy a scoped implication. ProofIR exhibit and fixed
harnesses carry `.provekit/config.toml` and
`.provekit/lift/<surface>/manifest.toml`; the runner invokes `provekit mint`
so the CLI, not the zoo, drives the native RPC lifter. Link exhibits invoke
`provekit link`, letting the Rust CLI derive cross-kit bridges and linker-error
receipts from the native kit streams. Scoped implications are handed back to
the CLI with `provekit prove --formula`.

The repeated `lab/`, `exhibit/`, and `fixed/` code is intentional. Each language
gets to use its own native surface; the runner only asks whether those surfaces
preserve the same content-addressed boundary under the kit projection.

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
| `BZ-SHAPE-007` | Rust callee contract plus Go cgo caller | `post_caller => pre_callee` | `provekit link` red/green link-bundle receipts |

## Run It

Run all species with the Bug Zoo runner:

```sh
cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all
```

or through the top-level target, which deliberately clears external CLI
overrides so the receipts are replayed against the current source tree:

```sh
make bug-zoo
```

Trace a slow or hanging run:

```sh
PROVEKIT_BUG_ZOO_TRACE=1 cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all --json
```

Trace output is written to stderr. The zoo logs host checks, `provekit mint`,
`provekit link`, and `provekit prove --formula` boundaries with elapsed time. It
also enables `PROVEKIT_CLI_TRACE=1` for spawned CLI work, which prints mint RPC
milestones for `initialize`, `lift`, and `shutdown` plus CLI-side link progress.
The JSON report includes the exact `provekit` command route used for each
species. By default the runner ignores `PROVEKIT_CLI` and invokes
`cargo run --manifest-path implementations/rust/provekit-cli/Cargo.toml -- ...`
so stale local binaries cannot stand in for the current code. To intentionally
test an external binary, set both `PROVEKIT_CLI=/path/to/provekit` and
`PROVEKIT_BUG_ZOO_EXTERNAL_CLI=1`; the report will mark the route as
`external-binary`.

Run discovery directly for the current TypeScript and C# null-boundary examples:

```sh
pnpm exec tsx menagerie/bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/tools/ts-boundary-discover.ts zod menagerie/bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/exhibit/zod/harness

dotnet run --project implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj -- discover csharp-linq menagerie/bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/exhibit/linq-where/harness
```

The discovery commands prove the language compiler/kit mapped source to an
addressable canonical object. The `provekit-bug-zoo` runner proves that the
specimen's native evidence preserves the expected shape under that projection,
then delegates scoped implication verdicts to `provekit prove --formula`.
