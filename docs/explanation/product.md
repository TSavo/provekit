# Sugar: Product

## What Sugar is

Sugar is a proof supply chain for code and packages that already exist.

Most projects already contain evidence about behavior: tests, assertions,
contracts, schemas, validators, type annotations, framework annotations, package
metadata, CI results, and proof-tool output. Today that evidence usually stays
inside one language, one test runner, one repository, or one build. Sugar
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

That is the important boundary. Sugar is not a verifier for one language and
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

Sugar makes the claims fight together. It lifts the local evidence into a
shared proof graph, composes the claims, and reports proof violations or
unresolved residue when the graph cannot justify the assembled system.

This is demonstrated end to end, not aspirational. A numpy vendor mints a
`.proof` carrying the callsite-keyed contract `np.add(2,3) == 5`. A consumer
stages that `.proof` in `.provekit/imports/`, asserts `np.add(2,3) == 6`, and
runs `prove`. The consumer is REFUSED: it inherited numpy's `== 5`, the verifier
conjoins the two same-callsite contracts, and z3 finds `and(== 5, == 6)` UNSAT. A
consumer that asserts `== 5` agrees and is PROVEN. The consumer inherits the
vendor's correctness and is caught contradicting it. This works because contracts
key to the callsite under test, not to the test, so a downstream assertion about
the same call meets the upstream contract about it.
(`implementations/python/provekit-lift-py-tests/tests/test_inheritance_e2e.py`,
parametrized `consumer-agrees-PROVEN` and `consumer-contradicts-REFUSED`; the
cross-proof conjoin is locked by `cross_proof_same_named_contracts_are_conjoined`
in `implementations/rust/provekit-verifier/src/consistency.rs`.)

That makes Sugar a supply-chain tool as much as a verification tool. The
question is not only "did this package pass its own checks?" The question is
"do the claims this package ships still hold when combined with the claims its
consumers, dependencies, bridges, and generated artifacts rely on?"

## `.proof` artifacts

A `.proof` artifact is a signed, content-addressed bundle of proof data. It can
contain contract mementos, source mementos, witness mementos, implication
witnesses, bridge attestations, proof-file conformance witnesses, materialization
receipts, package inspection claims, and policy-relevant metadata. Source and
witness mementos carry identity (CIDs plus loci plus signatures), not bodies; the
body is resolved on demand and recompute-verified. See
[proofchain.md](proofchain.md) for the Source Oracle and Witness Oracle.

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

## What Sugar replaces

Sugar replaces the missing portable layer under existing tools.

It does not replace `cargo test`, `npm test`, `go test`, JUnit, pytest, Kani,
Prusti, Coq, Lean, F*, Dafny, TLA+, Z3, schema validators, or static analyzers.
Those tools remain the source of evidence. Sugar gives their claims stable
bytes, CIDs, signatures, and a proof graph where downstream consumers can reuse
or reject them.

## What Sugar is not

Sugar is not a soundness-certified compliance product. If a deployment
requires a regulator-recognized proof artifact from Coq, Isabelle, F*, or a
specific certified toolchain, that toolchain remains the authority.

Sugar is not a substitute for runtime testing. Tests are often the evidence
that gets lifted. Adapter coverage is empirical, and each kit only sees the
idioms it knows how to walk.

Sugar is not a central registry. A `.proof` artifact verifies from its bytes,
CIDs, signatures, witnesses, and local policy. A server can index proof data for
discovery, but the server is not the source of truth.

Sugar is not a promise that every boundary can be materialized exactly.
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
