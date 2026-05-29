# ProvekIt: Product

## What ProvekIt is

ProvekIt is a proof supply chain for code and packages that already exist.

Most projects already contain evidence about behavior: tests, assertions,
contracts, schemas, validators, type annotations, framework annotations, package
metadata, CI results, and proof-tool output. Today that evidence usually stays
inside one language, one test runner, one repository, or one build. ProvekIt
promotes it into portable ProofIR or protocol claims, wraps those claims in
signed mementos, and distributes them as content-addressed `.proof` artifacts.

The product has two main actors:

- **Kits** know native ecosystems. They lift language-native evidence into
  ProofIR or protocol claims, materialize admitted claims back into native
  source when needed, and resolve dependency `.proof` artifacts through the
  package manager and filesystem rules of their ecosystem.
- **The CLI** computes over normalized proof data. It loads `.proof` artifacts,
  speaks RPC to kits, conjoins and composes claims, checks conformance, emits
  proof bundles, and proves obligations. It should not need to know Maven, npm,
  Cargo, Spring, Zod, JML, or Rust proc-macro semantics directly.

That is the important boundary. ProvekIt is not a verifier for one language and
not another test runner. It is a substrate where claims from many local tools
can be compared, composed, signed, and rejected when they contradict.

## The game changer

Local success does not imply assembled success.

A library can pass its tests. A consumer can pass its tests. A shim can pass its
smoke suite. The system assembled from those parts can still make inconsistent
claims: a callee publishes a weaker postcondition than the caller requires, two
packages disagree about a boundary value, a generated bridge drops a condition,
or a dependency update changes a contract set while the application still
expects the old one.

ProvekIt makes the claims fight together. It lifts the local evidence into a
shared proof graph, composes the claims, and reports proof violations or
unresolved residue when the graph cannot justify the assembled system.

That makes ProvekIt a supply-chain tool as much as a verification tool. The
question is not only "did this package pass its own checks?" The question is
"do the claims this package ships still hold when combined with the claims its
consumers, dependencies, bridges, and generated artifacts rely on?"

## `.proof` artifacts

A `.proof` artifact is a signed, content-addressed bundle of proof data. It can
contain contract mementos, implication witnesses, bridge attestations,
proof-file conformance witnesses, materialization receipts, package inspection
claims, and policy-relevant metadata.

Those artifacts form a DAG. Nodes and edges are named by content: contract CIDs,
attestation CIDs, contract-set CIDs, proof-bundle CIDs, binary CIDs, and protocol
catalog CIDs. If bytes change, the CID changes. Old mementos remain true about
the old bytes, but they do not silently apply to the new bytes.

This is where the cost model comes from:

- If a prior commitment is unchanged, a verifier may discharge work by CID
  equality, signature verification, and graph walking.
- If a signed implication already exists for a pair of claims, the verifier can
  reuse that implication instead of re-solving it.
- If a claim is new, changed, or newly composed in a way the DAG does not
  already justify, semantic proving still has to happen.

So the honest statement is not "all proving is O(1)." Expensive proof work can
be done once and then reused by content identity. New semantic work still costs
what it costs.

## Who it is for

**Library authors** can publish behavioral claims alongside package bytes. Their
tests, assertions, validators, contracts, and proof-tool outputs stay in the
native source. A kit lifts those surfaces and emits `.proof` artifacts.

**Application teams** can verify the claims that dependencies ship instead of
trusting a package because its upstream CI was green once. The verifier reports
which obligations discharged by equality, which used cached implications, which
needed proving, and which failed or remained unsupported.

**Kit authors** build the ecosystem-specific side: native syntax walking,
package proof resolution, framework sugar, diagnostic translation, and
materialization into the host language.

**Protocol and tooling authors** use the same substrate for claims that are not
ordinary function contracts: proof-file conformance, protocol catalog evolution,
CI input closure, generated repair closure, package inspection, and
materialization receipts.

## What ProvekIt replaces

ProvekIt replaces the missing portable layer under existing tools.

It does not replace `cargo test`, `npm test`, `go test`, JUnit, pytest, Kani,
Prusti, Coq, Lean, F*, Dafny, TLA+, Z3, schema validators, or static analyzers.
Those tools remain the source of evidence. ProvekIt gives their claims stable
bytes, CIDs, signatures, and a proof graph where downstream consumers can reuse
or reject them.

## What ProvekIt is not

ProvekIt is not a soundness-certified compliance product. If a deployment
requires a regulator-recognized proof artifact from Coq, Isabelle, F*, or a
specific certified toolchain, that toolchain remains the authority.

ProvekIt is not a substitute for runtime testing. Tests are often the evidence
that gets lifted. Adapter coverage is empirical, and each kit only sees the
idioms it knows how to walk.

ProvekIt is not a central registry. A `.proof` artifact verifies from its bytes,
CIDs, signatures, witnesses, and local policy. A server can index proof data for
discovery, but the server is not the source of truth.

ProvekIt is not a promise that every boundary can be materialized exactly.
Materialization can produce exact output, bounded lossy output with an explicit
loss record, or refusal. Silent loss is the forbidden case.

## Adoption surfaces

The current source-built CLI is:

```bash
cargo install --path implementations/rust/provekit-cli
provekit verify-protocol
```

A project that declares lift plugins can mint proof data:

```bash
provekit mint --project .
```

A consumer can run the proof gate over local and dependency `.proof` artifacts:

```bash
provekit prove .
```

The exact plugin manifests, kit coverage, and package-manager proof resolution
depend on the host ecosystem. Start with
[../quickstart-end-user.md](../quickstart-end-user.md) to run the CLI and
[../quickstart-extender.md](../quickstart-extender.md) to build or extend a kit.

## What you actually get

- A way to turn existing native evidence into portable ProofIR or protocol
  claims.
- Signed `.proof` artifacts that carry those claims by content identity.
- A language-agnostic CLI that composes and proves normalized proof data.
- Kit-owned integration with language syntax, libraries, package managers, and
  materialization surfaces.
- Reports that distinguish proved obligations, cached commitments, violations,
  unsupported residue, bounded loss, and refusal.
- A proof supply chain that runs over existing ecosystems instead of replacing
  them.
