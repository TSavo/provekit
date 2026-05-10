# ProvekIt: Prove `k(I)=t`

> *Supra omnia, rectum.*
>: T

ProvekIt is the geometry of how lossy abstract interpretations compose
into a sound joint inference over a content-addressed federated substrate.

The name is literal: **Prove `k(I)=t`**. ProvekIt is a general-purpose framework
for proving that a transformation `k`, applied to an input `I` from some domain,
produces the formal correctness representation `t`.

`k` can be a compiler, lifter, verifier, policy projector, protocol checker,
CI closure mapper, schema extractor, or repair transform. `I` can be source
code, annotations, tests, schemas, build inputs, proof files, package metadata,
or any other domain artifact. `t` is the canonical truth object the artifact is
supposed to yield: formal, content-addressed, signable, comparable, and
verifiable.

ProvekIt does not ask you to trust the artifact. It asks for signed,
content-addressed evidence that applying `k` to `I` produces `t`, then fails
closed when the graph does not carry the claim.

That linked evidence object is a **proofchain**: a locally verifiable chain of
signed, content-addressed evidence for logically true claims. A blockchain
carries state transitions; a proofchain carries formal proofs. Proof validity
does not need a global ledger because the object of verification is the
evidence itself.

Modern software already depends on `k(I)=t` claims everywhere. A compiler says
source becomes a binary. A type checker says a program inhabits a type. A CI run
says a precise closure of inputs produced a result. A schema says a payload has a
shape. A repair tool says a patch closes a defect.

Most of those claims are trusted because a tool said so, a log existed, a check
passed once, or a convention held locally. They do not travel cleanly across
languages, repositories, build systems, package ecosystems, generated code, and
time. The claim falls out of the place where it was made, and the next domain
has to trust it again from scratch.

Every test you have ever written is already a contribution to such a substrate.
Every type annotation, every assertion, every kernel-doc comment, every
OpenAPI schema, every Coq proof, every static-analyzer rule, every property
test: each one is a `k_i` that projects some lossy view of the same code into
its domain's expressible facts. They had nowhere to settle but their own
isolated checker. ProvekIt is the place where they conjoin. The substrate is
their joint inference: strictly more constraining than any single `k_i`,
content-addressed, federated across languages, monotonic under addition.

That is the thing software has never had: **a place where claims about behavior
settle once and apply everywhere.**

ProvekIt makes those claims first-class. The input, transformation, formal
truth object, evidence, and proof edge become content-addressed artifacts. They
can be signed, compared, composed, replayed, rejected, and carried across domain
boundaries without asking the next tool to inherit the previous tool's trust.

## The Correctness Stack

ProvekIt has three layers:

1. **Projection.** Native adapters apply `k` to domain artifacts `I`: source,
   annotations, tests, schemas, CI inputs, protocol files, package metadata, or
   generated repairs.
2. **Truth.** The result `t` is canonicalized into a formal claim object with
   stable bytes, stable CIDs, and explicit provenance.
3. **Proof.** The verifier decides whether the graph carries the required edge:
   an obligation is discharged, a missing implication is exposed, or a claimed
   closure is rejected.

That is the substrate. Software correctness stops being a local tool result and
becomes a portable, checkable relationship between domain evidence and formal
truth.

**Artifacts become accountable.** Code is one implementation of a claim, not the
claim itself. Refactoring, generated code, and AI-produced repairs can be judged
by the formal truth they preserve or fail to preserve.

**Domains compose.** A language contract, package policy, protocol conformance
claim, CI result, proof file, and repair witness can all live in the same graph
when they reduce to content-addressed truth objects.

**Correctness fails closed.** If the graph cannot prove the edge, the claim does
not travel. That is how bug classes become missing obligations instead of local
runtime surprises.

## Canonical Truth

Different domains need different projections, but the output of a projection
has to become something the rest of the graph can reason about. In this repo,
the central truth format is ProofIR.

ProofIR is not a universal language for re-expressing every implementation
detail of every programming language. It is a canonical language for claim
boundaries: preconditions, postconditions, invariants, protocol obligations,
value predicates, resource states, signer claims, CI blast radii, grammar
conformance claims, realizer outputs, and the implication edges that connect
them.

That is why a Spring annotation, a Zod validator, an OpenAPI schema, a Rust type
invariant, and a ProvekIt-native contract can all collapse to the same canonical
predicate when they assert the same boundary fact. The host-language texture can
be discarded; the obligation survives.

Once projected into ProofIR, a boundary is comparable, solvable, translatable,
content-addressable, and signable. It has canonical bytes and a CID. It can be
carried across languages, repositories, package ecosystems, commits, and time.
The contracts were often already in your code; ProvekIt turns them into
accountable edges the rest of the graph must satisfy.

## I want to...

| | |
| --- | --- |
| **Use the CLI** | [docs/quickstart-end-user.md](docs/quickstart-end-user.md) to install and run `provekit`; [docs/reference/protocol-extensions.md#tool-surfaces](docs/reference/protocol-extensions.md#tool-surfaces) for the command surface |
| **See a bug class map to an addressable shape CID across languages** | [docs/explanation/bug-zoo.md](docs/explanation/bug-zoo.md); run `cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all` |
| **See supported languages and kit coverage** | [docs/reference/per-language-status.md](docs/reference/per-language-status.md) |
| **Understand the move** | [docs/papers/](docs/papers/): recommended order: paper 03 → 06 → 02 |
| **Understand proofchains** | [docs/explanation/proofchain.md](docs/explanation/proofchain.md) |
| **Extend it / build a kit** | [docs/contributing/](docs/contributing/) |
| **Read the spec** | [docs/papers/02-bluepaper.md](docs/papers/02-bluepaper.md) |
| **Understand the new protocol/tooling surface** | [docs/reference/protocol-extensions.md](docs/reference/protocol-extensions.md) |
| **Bind CI results to supply-chain inputs** | [docs/how-to/content-addressed-ci.md](docs/how-to/content-addressed-ci.md) |
| **Compare to other tools** | [docs/explanation/compared-to/](docs/explanation/compared-to/) |

For more entry points (per-language tutorials, IDE integration, publishing a `.proof`, CICP, Bug Zoo, protocol extensions, threat model, and spec CIDs), see [docs/index.md](docs/index.md).

## Status

- **Protocol catalog**: v1.6.2 (patch over v1.6.1; catalogs the Content-Addressed CI Protocol as an extension-only protocol)
- **Catalog CID**: `blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f`
- **Canonical implementation**: Rust, built from this repository with `cargo install --path implementations/rust/provekit-cli`
- **Conforming implementations**: Rust, TypeScript, Python, Java, C#, Ruby, Zig, Go, C++, Swift, C, PHP. Coverage varies; see [docs/reference/per-language-status.md](docs/reference/per-language-status.md).
- **Protocol evolution**: PEP dogfoods catalog transitions as signed, content-addressed body-claims under `protocol/evolution/v1.6.1/` and `protocol/evolution/v1.6.2/`.
- **Content-addressed CI**: CICP binds CI results to exact source, protocol catalog, kit/toolchain, config, and accepted witness inputs. Reuse is allowed only when that closure is byte-identical.
- **Bug Zoo**: the self-contained `menagerie/bug-zoo/` runner checks lab, exhibit, fixed, link, equivalence, and composition receipts for checked-in specimens. Wild sightings are metadata only until real upstream specimens are pinned and wired into the runner.
- **Menagerie**: [menagerie/](menagerie/) is the executable map of proof workflows. Bug Zoo is the runnable destination today; Hashbound Mainline, Supply Chain Rails, Bridgeworks, Protocol Switchyard, and Change Station name the next routes.
- **Conformance gate**: catalog CIDs, proof-protocol fixtures, CICP vectors, self-contract attestations, and per-kit tests must agree before CI is green.

The protocol is content-addressed end to end. Each version's canonical name is its own catalog hash. Anyone with the spec bytes can verify that label locally. No central party decides what a version means; the bytes do.

## Bug Zoo

Bug Zoo is the executable lab for the claim above. Each specimen runs in an
isolated host-language environment, uses that language's own compiler/kit to
map source to canonical truth, then checks that the expected shape CID or
boundary receipt CID is addressable from that projection. The normal proof gate
for projects is `provekit prove`; Bug Zoo owns the fixture orchestration under
`menagerie/bug-zoo/` and routes lift, link, and proof work through the Rust CLI.
It is the first runnable destination in the broader [Menagerie](menagerie/),
where workflows like Hashbound Mainline, Supply Chain Rails, Bridgeworks,
Protocol Switchyard, and Change Station can share the same proof-carrying
shape.

The zoo is organized by species, not by language. A species directory owns a
`specimen.yaml` manifest, then each language under that species carries the same
lifecycle:

- `lab/`: ordinary host code that passes native checks while the bug class is
  still latent. It has no ProvekIt workflow.
- `exhibit/<surface>/`: a native contract surface that lifts or links to the
  missing edge and yields the red `provekit prove` or `provekit link` signal.
- `fixed/<surface>/`: the paired source with that boundary closed, re-run
  through the same surface to yield the green `provekit prove` or
  `provekit link` signal.
- `wild/`: optional real upstream sightings pinned by advisory, commit, path,
  and evidence. No checked-in wild specimens are executed today; current
  `wildSightings` entries are reported as metadata.

In shorthand:

```text
k_lang(I) = t
```

`k_lang` is the language compiler as a ProvekIt kit/lifter, `I` is the source,
and `t` is the canonical truth object: a ProofIR shape CID for claim
boundaries, or a LinkBundle receipt CID for cross-kit bridge derivations.
Different languages can disagree in syntax, runtime behavior, and exception
type while their native evidence maps homomorphically to the same addressable
shape.

Each native surface maps through a structure-preserving homomorphism into the
correctness object; the proof layer checks whether the mapped obligation
commutes with equivalent surfaces or closes under the fixed witness.

The current null-boundary receipts show Java, TypeScript, and C# lifting the same
missing edge:

```text
maybe_null(name) => non_null(name)
```

to the same ProofIR CID:

```text
blake3-512:0d611d8478a205ff040e7d0bcf6c21b12051340ecc5f00c3953af632b23fc01e069b4ad8a8699869163e135b9fde85792eba6acc54cd75cb3d3cc6a40a99ded4
```

They also run the red/green proof obligations through the Rust CLI: lab null is
rejected against each lifted non-null requirement, and each fixed surface
discharges the paired non-null implication with `provekit prove --formula`.

Bug Zoo also carries value-scope escape as `BZ-SHAPE-006`: Java JUnit and Spring
exhibits both witness a point value, and the runner invokes
`provekit prove --formula` to produce the red signal when 42 fails a `>= 43`
requirement and the green signal when the fixed surface witnesses 43.

`BZ-SHAPE-007` carries the polyglot link-obligation specimen: a Go cgo caller
invokes a Rust callee whose native contract requires a stricter input. The zoo
routes the fixture through `provekit link`; the exhibit produces an
`unprovable-obligation` link-bundle receipt, and the fixed pair links clean.

Read [docs/explanation/bug-zoo.md](docs/explanation/bug-zoo.md), or run:

```sh
cargo run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all
```

| Kit | Self-contracts | Lift-plugin-protocol bridges | LSP plugin |
|---|---|---|---|
| Rust | full conformance | full (source of truth) | shipping |
| Go | full conformance | in progress | planned |
| C# | full conformance | not started | shipping |
| Ruby | in progress | not started | shipping |
| Zig | in progress | not started | shipping |
| Python | full conformance | in progress | shipping |
| TypeScript | full conformance | in progress | planned |
| C++ | full conformance | not started | planned |
| Java | full conformance | not started | planned |
| Swift | full conformance | not started | planned |
| C | full conformance | not started | planned |
| PHP | in progress | not started | planned |

## Install

This project is **build-from-source only**. Crates.io publishing is on the roadmap; until then see [docs/quickstart-end-user.md](docs/quickstart-end-user.md) for build instructions.

The core binary is:

```sh
cargo install --path implementations/rust/provekit-cli
```

`provekit verify-protocol` confirms the local install conforms to the expected protocol catalog CID. `cargo provekit-lift` walks the workspace, runs every registered lift adapter, and emits a `.proof` catalog of signed contract mementos. `provekit prove` runs the three-tier handshake and reports the discharge breakdown. `provekit proof`, `provekit protocol`, and `provekit ci` cover proof-file conformance, PEP transitions, and CICP supply-chain admission. Bug Zoo specimens are checked by the repo-owned runner under `menagerie/bug-zoo/`. Any of these can fail closed; none requires the network.

For other host languages, see the polyglot-stack tutorial above. The Rust CLI is the canonical implementation; non-Rust kits use it for verification today.

## Building from source

If you are working on ProvekIt itself (kit, lift adapter, prover backend, spec change), see [docs/contributing/build.md](docs/contributing/build.md) for the polyglot Make targets, system dependencies, and per-implementation build commands. The default `make ci` gate covers the Linux conformance profile plus the Linux native test aggregate; the full GitHub workflow adds macOS Swift and per-kit verifier jobs.

## License

Source files use SPDX headers where present. A repository-level license file has not been added yet.
