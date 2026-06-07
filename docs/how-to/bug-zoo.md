# Bug Zoo

Bug Zoo is the executable proof that Sugar's bug story is not just a slogan.

A normal bug example shows vulnerable code and a patch. Bug Zoo turns that into a stronger, content-addressed receipt: the lab code passes its ordinary host checks without a Sugar workflow, the exhibit surface lifts the latent boundary or validates a checked-in LinkBundle receipt, and the paired fixed artifact is accepted only after the same surface re-runs and produces a green signal.

This matters because it makes software correctness across domains concrete. A Spring annotation, a Sugar-native Java contract, a JUnit assertion, a Zod schema, an OpenAPI rule, or a historical OSS patch can all point at the same boundary obligation once lifted. If the boundary is real, its projected shape gets a CID. If a lifter misses it, the specimen fails. If a fixed surface is plausible but does not close the edge, the specimen fails.

Bug Zoo is not a patch archive. Historical fixes are context. The durable claim is independent rediscovery and verified closure.

## The Receipt

The default null-boundary specimen is:

```text
menagerie/bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence
```

It starts with ordinary code that passes host checks. Its exhibits show the same non-null boundary through several source surfaces:

- Sugar-native Java annotations and Spring Web `@RequestParam`;
- TypeScript zod and class-validator;
- C# DataAnnotations, `//provekit:` annotations, and LINQ.

Those are not the same host program. They project to the same contract boundary after lifting: `neq(name, null)`.

The missing edge is:

```text
maybe_null(name) => non_null(name)
```

The lab witness for null is not a Sugar artifact; it is the bug shape. The
exhibit/fixed receipt is live: each native surface is lifted with
`provekit mint`, then the runner asks the verifier formula gate to reject the
lab null witness against the lifted non-null requirement and to discharge the
paired fixed non-null witness.

The zoo also carries `BZ-SHAPE-006-value-scope-escape`, where Java has two exhibits under the same specimen: JUnit and Spring. Both witness a point value. The composition receipt is a verifier formula-gate result: a witness for 42 does not satisfy a `>= 43` requirement, while both fixed surfaces witness 43 and satisfy it.

`BZ-SHAPE-007-polyglot-link-obligation` is the polyglot specimen. A Go cgo
caller reaches a Rust callee with a native precondition, and the runner validates
the checked-in LinkBundle receipts for the cross-kit bridge and link-bundle
verdict.

## Lifecycle

Each species can carry four states:

- `lab/`: ordinary host-language code and checks. These should pass, because the bug is latent under the host's normal surface. No `.provekit` project is configured here.
- `exhibit/`: one or more source surfaces lift into ProofIR or link into a LinkBundle and expose the missing edge with a red CLI signal.
- `fixed/`: paired source surfaces close the edge and re-run through the same ProofIR or LinkBundle path with a green CLI signal.
- `wild/`: optional real upstream sightings pinned by advisory, commit, path,
  and evidence. Today this is metadata: the runner reports `wildSightings`, but
  no checked-in `wild/` specimens are executed.

## Run It

Run the default null-boundary specimen:

```sh
cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- menagerie/bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence
```

Run the value-scope specimen explicitly:

```sh
cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- menagerie/bug-zoo/species/BZ-SHAPE-006-value-scope-escape
```

Run the polyglot link specimen explicitly:

```sh
cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- menagerie/bug-zoo/species/BZ-SHAPE-007-polyglot-link-obligation
```

Run every specimen:

```sh
cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all
```

Emit structured output:

```sh
cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all --json
```

Trace a slow or hanging specimen:

```sh
PROVEKIT_BUG_ZOO_TRACE=1 cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all --json
```

Trace output goes to stderr. It prints each host check, `provekit mint`,
checked-in LinkBundle receipt read, and verifier formula-gate boundary with elapsed time.
During mint, the runner also enables `PROVEKIT_CLI_TRACE=1`, so the Rust CLI
prints RPC milestones for `initialize`, `lift`, and `shutdown`.

## What The Checker Verifies

For each specimen, the self-contained Bug Zoo runner checks:

- the host check command in `specimen.yaml` passes;
- the lab state remains host-only, with no Sugar lifter/prover workflow;
- every exhibit RPC returns ProofIR whose CID matches the checked-in expected ProofIR;
- every link exhibit returns a LinkBundle whose CID and linker errors match the checked-in receipt;
- exhibit diagnostics mention the declared missing edge;
- every fixed RPC returns ProofIR whose CID matches the checked-in expected fixed ProofIR;
- every fixed link exhibit returns a clean LinkBundle whose CID matches the checked-in receipt;
- fixed diagnostics are clean;
- required exhibit pairs are boundary-equivalent by ProofIR CID;
- composition checks invoke the verifier formula gate and reject or satisfy scoped implications as declared;
- the expected Sugar verification failure is present before closure;
- the paired fixed surface closes the edge after re-lift or re-link.

## Trust Model

A source surface is not trusted because it looks like a contract. A fixed surface is not trusted because it looks like a patch. Either one becomes evidence only after the host artifact is re-lifted or re-linked and the resulting receipt matches the named boundary behavior.

That is the important distinction: host syntax is a candidate, while the post-projection boundary receipt is the claim the substrate can carry.

## Specimen Shape

The manifest is `specimen.yaml`. Important fields:

- `predicates.boundary`, `predicates.sink`, `predicates.missingEdge`;
- `languages[].commands.hostCheck`, which is the lab's host-only gate;
- `languages[].exhibits[]`, each with a harness Sugar project, expected ProofIR file, diagnostic file, fixed pair, and lossiness note;
- `languages[].linkExhibits[]`, each with a harness project, expected LinkBundle file, diagnostic file, optional kit RPC binary, fixed pair, and lossiness note;
- `languages[].equivalence.required`, naming exhibit pairs that must lift to the same boundary CID;
- `languages[].composition.checks`, which are the red/green proof obligations routed through the verifier formula gate.

The root [../../menagerie/bug-zoo/README.md](../../menagerie/bug-zoo/README.md) explains the lifecycle. The worked species live under [../../menagerie/bug-zoo/species/](../../menagerie/bug-zoo/species/).
