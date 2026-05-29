# ProvekIt

ProvekIt is a proof supply chain for software that already exists.

It does not ask every team to rewrite code in a proof language. It takes the
evidence teams already maintain in their native ecosystems, including tests,
assertions, contracts, schemas, validators, framework annotations, and
boundary/library sugar, and turns that evidence into portable ProofIR claims.
Those claims are packaged as signed, content-addressed `.proof` artifacts that
other packages, tools, and languages can verify without trusting the original
test runner or re-running the original proof every time.

The important failure mode is composition. A package can pass its tests, another
package can pass its tests, and the assembled system can still contain
contradictory behavioral claims. ProvekIt makes those claims meet in one proof
graph. The CLI conjoins and composes normalized claims, proves the obligations
it can prove, and reports proof violations or unresolved residue where the
assembled claims do not fit together.

## The Shape

ProvekIt has two deliberately separate responsibilities.

**Kits own language reality.** A kit knows the language, package manager,
compiler, test framework, annotation library, and ecosystem conventions. A Rust
kit can walk Rust tests and contracts. A Java kit can read Bean Validation, JML,
Spring, JUnit, and Maven-shaped package data. A TypeScript kit can read Zod,
class-validator, fast-check, and npm-shaped package data. Kits lift native
evidence into ProofIR or protocol claims, materialize admitted claims back into
native source when a workflow calls for it, and resolve dependency `.proof`
artifacts through the package manager and filesystem rules of their ecosystem.

**The CLI owns proof computation over normalized data.** The `provekit` CLI is
language-agnostic. It loads `.proof` artifacts, speaks RPC to configured kits,
normalizes claims, composes implications, checks proof-file conformance, emits
proof bundles, and proves obligations. The CLI should not need to know what a
Maven classifier, npm workspace, Rust proc macro, or Spring annotation means.
The kit translates those surfaces to proof data; the CLI computes over the
proof data.

That boundary is the product. ProvekIt is not "a better Rust verifier" or "a
new test runner." It is the place where language-native evidence becomes a
portable claim that can survive package boundaries.

## Why It Matters

Software supply chains already contain evidence, but the evidence is trapped in
local tools.

- A passing test suite says something true about one package at one time.
- A contract annotation says something true in one language's syntax.
- A schema validator says something true at one boundary.
- A formal verifier says something true in one proof system.
- A CI run says something true about one input closure.

ProvekIt gives those results a common substrate. Once a kit lifts the evidence
to canonical ProofIR or a protocol claim, the claim has stable bytes, a CID, a
signature, and a place in a `.proof` DAG. A downstream verifier can compare the
claim by CID, check a signed implication, compose it with other claims, or run a
semantic prover when a new obligation actually needs proof.

This distinction matters:

- Previously minted, unchanged commitments can often be verified cheaply by CID
  equality, signature checks, and graph walking.
- Semantic proving still happens when claims are minted, changed, or newly
  composed in a way the DAG does not already justify.
- ProvekIt does not make all proving constant-time. It amortizes expensive proof
  work by making prior commitments content-addressed and reusable.

The result is a proof supply chain over existing package ecosystems. Package
authors keep their normal tools. Consumers get portable claims about the code
they assemble.

## The `.proof` DAG

A `.proof` artifact is a signed, content-addressed bundle of proof data. It can
contain contract mementos, implication witnesses, bridge attestations,
proof-file conformance witnesses, materialization receipts, package inspection
claims, and policy-relevant metadata.

The graph is a DAG because every claim names the exact content it depends on:
contract CIDs, attestation CIDs, contract-set CIDs, proof-bundle CIDs, binary
CIDs, and protocol catalog CIDs. Old facts remain true about the old bytes that
minted them. When code or claims change, new CIDs appear. Nothing needs a
central invalidation service; unchanged commitments remain checkable by content
identity.

That is the supply-chain move:

```text
native evidence -> kit lift/materialize -> ProofIR or protocol claim
               -> signed memento -> .proof DAG -> provekit proof computation
```

## What The CLI Does

The canonical CLI is the Rust `provekit` binary. Current subcommands include
proof and protocol surfaces such as:

- `provekit mint`: dispatch configured lift plugins and write `.proof`
  artifacts.
- `provekit prove`: load `.proof` artifacts, resolve dependency proofs through
  kits, enumerate obligations, compose/conjoin claims, and report discharge
  status.
- `provekit verify`: verify a kit end to end by lifting contract claims,
  discharging each claim, and emitting per-claim receipts.
- `provekit proof`: hash, inspect, check, and witness `.proof` conformance.
- `provekit protocol`: work with protocol catalog evolution artifacts.
- `provekit materialize`: route concept or boundary carriers through native
  realize kits and emit admitted source plus proof receipts.
- `provekit link` and `provekit compose`: derive and compose cross-contract
  bridges.
- `provekit verify-protocol`: confirm the local binary conforms to its embedded
  protocol catalog CID.

The command surface keeps moving as protocol work lands. Use `provekit --help`
and [docs/reference/per-language-status.md](docs/reference/per-language-status.md)
for the current matrix.

## Install

This repository is build-from-source today. Crates.io publishing is still future
work. The current install path is:

```sh
cargo install --path implementations/rust/provekit-cli
```

Verify the installed CLI's embedded protocol catalog:

```sh
provekit verify-protocol
```

For project setup and first runs, start with
[docs/quickstart-end-user.md](docs/quickstart-end-user.md). If you are working
on ProvekIt itself, see [docs/contributing/build.md](docs/contributing/build.md)
for the polyglot Make targets, system dependencies, and per-implementation build
commands.

## Current Status

- **Canonical implementation:** the Rust CLI in
  `implementations/rust/provekit-cli`.
- **Current CLI protocol catalog:** v1.6.6, embedded in the CLI and verified by
  `provekit verify-protocol`.
- **Supported ecosystem surface:** Rust, TypeScript, Python, Java, C#, Ruby,
  Zig, Go, C++, Swift, C, and PHP have varying kit, library, lift-adapter,
  embedded-verifier, and LSP coverage. See
  [docs/reference/per-language-status.md](docs/reference/per-language-status.md).
- **Proof artifacts:** `.proof` envelopes, signed mementos, contract CIDs,
  attestation CIDs, contract-set CIDs, and protocol catalog CIDs are the durable
  units.
- **Executable exhibits:** [menagerie/bug-zoo/](menagerie/bug-zoo/) runs checked
  specimens that show native checks passing while lifted cross-package or
  cross-language obligations fail until the missing edge is closed.
- **Self-application:** the CLI can mint proof data from its own assertions and
  tests; see
  [docs/self-application/2026-05-28-snake-eats-tail.md](docs/self-application/2026-05-28-snake-eats-tail.md).

## Start Here

| Goal | Read |
|---|---|
| Install and run the CLI | [docs/quickstart-end-user.md](docs/quickstart-end-user.md) |
| Understand the product surface | [docs/explanation/product.md](docs/explanation/product.md) |
| Understand the architecture | [docs/explanation/architecture.md](docs/explanation/architecture.md) |
| Understand `.proof` and proofchains | [docs/explanation/proofchain.md](docs/explanation/proofchain.md) |
| See kit and language coverage | [docs/reference/per-language-status.md](docs/reference/per-language-status.md) |
| Publish a `.proof` artifact | [docs/how-to/publishing-a-proof.md](docs/how-to/publishing-a-proof.md) |
| Build or extend a kit | [docs/quickstart-extender.md](docs/quickstart-extender.md) |
| Compare to other tools | [docs/explanation/compared-to/](docs/explanation/compared-to/) |
| Read the paper ladder | [docs/papers/README.md](docs/papers/README.md) |

For the full docs map, see [docs/index.md](docs/index.md).

## What ProvekIt Is Not

ProvekIt is not a replacement for tests. Tests remain the source of much of the
evidence that kits lift.

ProvekIt is not a replacement for Kani, Prusti, Coq, Lean, F*, Dafny, TLA+, Z3,
or other verification tools. Those tools can produce evidence; ProvekIt gives
that evidence a portable content-addressed supply chain.

ProvekIt is not a central registry. `.proof` artifacts verify from their bytes,
CIDs, signatures, witnesses, and local policy. A server may index proof data for
convenience, but it is not the authority.

ProvekIt is not a promise that any current kit sees every useful contract in a
codebase. Adapter coverage is empirical. Unknown, unsupported, or lossy
surfaces must be reported honestly as residue, loss, or refusal.

## License

Source files use SPDX headers where present. A repository-level license file has
not been added yet.
