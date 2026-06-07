# Sugar Architecture

Sugar is a pipeline for turning native software evidence into portable proof
data, then computing over that proof data without keeping language-specific
logic in the core CLI.

The short version:

```text
host evidence -> kit lift -> ProofIR/protocol claim -> signed memento
             -> .proof DAG -> provekit compose/conjoin/prove/report
```

## The boundary

The architecture is organized around one boundary.

**Kits own native meaning.** A kit may know a language grammar, an AST library,
a compiler or bytecode format, a package manager, a test framework, an
annotation library, a validation library, or an IDE surface. Kits are allowed to
understand Cargo, Maven, npm, Spring, JML, JUnit, Zod, Pydantic, Rust contracts,
or any other host-specific surface. They lift those surfaces into normalized
claims, materialize admitted claims back into native source when asked, and
resolve dependency `.proof` artifacts in the way their ecosystem actually
ships packages.

**The CLI owns normalized proof computation.** The Rust `provekit` CLI loads
`.proof` artifacts, speaks JSON-RPC style plugin protocols to configured kits,
checks proof-file and protocol conformance, composes and conjoins claims,
dispatches prover work, emits proof bundles, and reports the result. It should
not need to understand every package manager or annotation library. It computes
over ProofIR, protocol claims, CIDs, signatures, witnesses, and policy.

This keeps the system extensible. Adding a language or package ecosystem should
add a kit, not a new proof engine.

## Claims

The central normalized claim format is ProofIR. ProofIR is not a universal
language for re-expressing every implementation detail of every programming
language. It is a canonical language for claim boundaries: preconditions,
postconditions, invariants, value predicates, protocol obligations, bridge
edges, materialization receipts, and the implication edges that connect them.

Host-language texture can be discarded when it is not part of the obligation.
The obligation survives as canonical bytes. Those bytes can be hashed, signed,
compared, solved, transported, and packaged.

Protocol claims use the same substrate pattern. A proof-file conformance claim,
a protocol-catalog evolution claim, a package inspection claim, and a
materialization receipt are not all ordinary function contracts, but they still
become content-addressed claims with witnesses and policy.

## `.proof` DAGs

A `.proof` artifact is the transport container for signed proof data. It is not
a replacement for `Cargo.toml`, `package.json`, `pom.xml`, or other native
manifests. Those ecosystems still own package distribution. A `.proof` artifact
travels with or beside those packages and carries the claims the package is
willing to make.

Inside a `.proof` artifact, members are content-addressed. The DAG can include:

- contract mementos;
- source mementos (a locus plus pinned CIDs, not the source body);
- witness mementos (a signed pointer to a separately deployed witness body);
- implication witnesses;
- bridge attestations;
- proof-file conformance witnesses;
- package inspection claims;
- materialization or emit receipts;
- binary, contract-set, attestation, and protocol catalog CIDs.

Source and witness mementos carry IDENTITY, not bodies. The body is resolved on
demand and recompute-verified when a verifier needs it; see
[proofchain.md](proofchain.md) for the Source Oracle and Witness Oracle.

The verifier walks this DAG under policy. If a node or edge names bytes that are
not present, malformed, unsigned, signed by an untrusted key, or semantically
insufficient, the claim does not travel.

## Proof computation

For a composed obligation, the CLI uses the cheapest honest route available.

1. **CID equality.** If two canonical claims are byte-identical, their CIDs
   match. This discharges identity cases without theorem proving.
2. **Cached implication.** If a signed implication memento already proves that
   one claim implies another, the verifier can check the memento and reuse that
   result.
3. **Semantic proving.** If the graph does not already carry the edge, a prover
   has to prove or reject the new obligation. If accepted, the result can be
   minted as a new memento for future reuse.

This is an amortization model, not a claim that all proof is constant-time.
Previously minted and unchanged commitments are cheap to recheck. Newly minted,
changed, or newly composed semantic claims still require semantic work.

## Composition

Traditional local verification asks whether one artifact satisfies one local
contract. Sugar asks whether the assembled claim graph carries the edges the
consumer needs.

That matters because local success can compose into global contradiction. A
test suite can pass while a dependency publishes a weaker guarantee than the
consumer assumes. A bridge can type-check while dropping a boundary condition. A
generated artifact can compile while failing to re-lift to the claim it was
supposed to realize.

Sugar handles this by conjoining and composing normalized claims. If the
assembled graph cannot satisfy the obligation, the result is a proof violation,
unresolved residue, explicit bounded loss, or refusal, not a silent pass.

### Cross-proof contract conjoin

Contracts key to the CALLSITE under test, not to the test that exercised them
(for example `numpy.add#euf#...::assertion`). The contract name is the
content-keyed callsite, so same name means same callsite. Before the
satisfiability check, the verifier groups same-named contracts across all loaded
`.proof` files, including imported dependency proofs staged in
`.provekit/imports/`, and conjoins their invariants. Same name means sound to
conjoin.

This is the mechanism behind inherited correctness. A consumer that imports a
numpy `.proof` asserting `np.add(2,3) == 5` and then asserts `np.add(2,3) == 6`
lands both contracts on the same callsite name. Conjoining gives
`and(== 5, == 6)`, which z3 finds UNSAT, so the consumer is refused for
contradicting the contract it inherited. A consumer that asserts `== 5` agrees
and is proven. Identical assertions dedupe by CID and a lone contract is
untouched, so consistent compositions stay proven and there is no false refusal.
(`implementations/rust/provekit-verifier/src/consistency.rs:278-300`, locked by
the `cross_proof_same_named_contracts_are_conjoined` test; end-to-end in
`implementations/python/provekit-lift-py-tests/tests/test_inheritance_e2e.py`.)

## Kit RPC

Kit interaction is explicit. Current CLI paths dispatch configured plugins for
lift, emit, materialize, package inspection, dependency proof resolution, and
related surfaces. The plugin protocol uses request/response methods such as
`provekit.plugin.invoke`, `provekit.plugin.assemble`,
`provekit.plugin.resolve_dependency_proofs`, and shutdown handshakes over the
configured subprocess transport.

The design goal is simple: native knowledge stays in the kit, proof computation
stays in the CLI, and the exchange between them is normalized, content-addressed
proof data.

### The kit oracle is untrusted

When a verifier needs a body that the `.proof` only points at, it calls the kit
oracle over RPC (for example `provekit.plugin.resolve_witness`). That oracle is
UNTRUSTED. It RESOLVES bytes; it does not pronounce verdicts. The Rust CLI
verifies the Ed25519 signature itself with the substrate's own primitive
(`ed25519_verify_string`), then BLAKE3's the resolved bytes itself and compares
to the pinned CID. A body the oracle returns that does not recompute to the
pinned CID is a broken oracle, caught because Rust does the math anyway. A body
that recomputes but whose honest re-run differs is drift. Both refuse loudly and
are distinguished. The rule is trust the recomputation, never the resolver.
(`implementations/rust/provekit-cli/src/witness_verify.rs:1-18`.)

This is why verification lives in the language-blind CLI and not in the kit: the
kit knows the language, but it must not be trusted to grade itself.

## Fail-closed posture

Every public proof surface should fail closed:

- malformed `.proof` bytes are rejected;
- recomputed CIDs must match the claimed CIDs;
- signatures must verify under local policy;
- dependency proof paths returned by kits must exist and be `.proof` artifacts;
- materialized output must carry receipts or explicit refusal/loss;
- unknown or unsupported native surfaces remain residue instead of becoming
  invented claims;
- prover timeout or absence is not a proof.

This posture is what makes the supply-chain story honest. The graph either
carries the claim under policy or it does not.

## Read further

- [product.md](product.md) for the product framing.
- [proofchain.md](proofchain.md) for the linked evidence object.
- [lift-not-author.md](lift-not-author.md) for the adoption posture.
- [cross-language-equivalence.md](cross-language-equivalence.md) for the
  concept-hub and morphism model.
- [../reference/per-language-status.md](../reference/per-language-status.md) for
  current kit and adapter coverage.
- [../../protocol/specs/](../../protocol/specs/) for the canonical specs.
