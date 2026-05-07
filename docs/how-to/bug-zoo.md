# Bug Zoo

Bug Zoo is the executable proof that ProvekIt's bug story is not just a slogan.

A normal bug example shows vulnerable code and a patch. Bug Zoo turns that into a stronger, content-addressed receipt: the code passes its ordinary host checks, ProvekIt lifts the latent boundary, the verifier exposes the missing `p => q` edge, and any generated repair is accepted only after the repaired artifact re-lifts to a closed graph.

This matters because it makes software correctness across domains concrete. A Spring annotation, a ProvekIt-native Java contract, a Zod schema, an OpenAPI rule, or a historical OSS patch can all point at the same boundary obligation once lifted. If the boundary is real, it gets canonical bytes and a CID. If a lifter misses it, the specimen fails. If a dropper emits plausible code that does not close the edge, the specimen fails.

Bug Zoo is not a patch archive. Historical fixes are context. The durable claim is independent rediscovery and verified closure.

## The Receipt

The default specimen is:

```text
bug-zoo/species/BZ-SHAPE-005-java-null-boundary-equivalence
```

It starts with ordinary Java that passes its host checks. The exposed variants show the same non-null boundary through two source surfaces:

- ProvekIt-native Java annotations;
- Spring Web `@RequestParam`.

Those are not the same Java program. They are the same contract boundary after lifting: `neq(name, null)`.

The missing edge is:

```text
maybe_null(name) => non_null(name)
```

The dropped variant uses a Java ORP realizer to emit a native edge-closing shape. That generated source is only a candidate until re-lift produces the expected closure ProofIR and the fix receipt binds the source artifact, transformed artifact, post-lift CID, closure witness CID, closure ProofIR CID, target symbol, missing edge, and policy.

## Lifecycle

Each species can carry four states:

- `lab/`: ordinary host-language code and checks. These should pass, because the bug is latent under the host's normal surface.
- `exposed/`: one or more source surfaces lift into ProofIR and expose the missing edge.
- `dropped/`: a proof-first realizer/dropper emits a host artifact candidate, then ProvekIt re-lifts it and verifies closure.
- `wild/`: real upstream sightings pinned by advisory, commit, path, and evidence.

## Run It

Run the default Java specimen:

```sh
cargo run --manifest-path bug-zoo/Cargo.toml -- bug-zoo/species/BZ-SHAPE-005-java-null-boundary-equivalence
```

Run one specimen explicitly:

```sh
cargo run --manifest-path bug-zoo/Cargo.toml -- bug-zoo/species/BZ-SHAPE-006-typescript-null-boundary-equivalence
```

Run every specimen:

```sh
cargo run --manifest-path bug-zoo/Cargo.toml -- --all
```

Emit structured output:

```sh
cargo run --manifest-path bug-zoo/Cargo.toml -- --all --json
```

## What The Checker Verifies

For each specimen, the self-contained Bug Zoo runner checks:

- the host check command in `specimen.yaml` passes;
- every exposure RPC returns ProofIR whose CID matches the checked-in expected ProofIR;
- exposure diagnostics mention the declared missing edge;
- required exposure pairs are boundary-equivalent by ProofIR CID;
- the expected ProvekIt verification failure is present before closure;
- optional dropper output is accepted only after the generated artifact is re-lifted.

For dropped specimens, the checker also verifies:

- the realizer RPC returns `status: "closed"`;
- `modifiedSource` matches the checked-in dropped output source;
- `postLift.ir` hashes to the checked-in closure ProofIR;
- the closure witness body and CID are recomputed;
- the fix receipt binds the source artifact CID, transformed artifact CID, post-lift CID, closure witness CID, closure ProofIR CID, surface, target symbol, and missing edge;
- `proof-plan.json` and `language-dropper.json`, when present, bind to the same fix receipt.

## Trust Model

A realizer is not trusted because it produced code. A dropper is not a proof. A dropper output becomes evidence only after the transformed host artifact is re-lifted and the resulting ProofIR closes the named missing edge under policy.

That is the important distinction: generated code is a candidate, while the post-lift closure witness is the claim the substrate can carry.

## Specimen Shape

The manifest is `specimen.yaml`. Important fields:

- `predicates.boundary`, `predicates.sink`, `predicates.missingEdge`;
- `commands.hostCheck`;
- `exposures[]`, each with a lifter RPC, expected ProofIR file, diagnostic file, and lossiness note;
- `equivalence.required`, naming exposure pairs that must lift to the same boundary CID;
- `dropper`, naming the realizer RPC, source, output source, proof plan, language projection, closure ProofIR, and fix receipt.

The root [../../bug-zoo/README.md](../../bug-zoo/README.md) explains the lifecycle. The Java, TypeScript, and C# null-boundary specimens each have worked READMEs under [../../bug-zoo/species/](../../bug-zoo/species/).
