# Sugar: a proof supply chain for existing software

A modern Rust workspace pulls in a few hundred crates. A modern npm project pulls in tens of thousands. A modern Go service drags in transitive C dependencies through cgo. Behind every `cargo build`, `npm install`, and `go mod tidy`, a developer is implicitly trusting a graph of code they have never read, written by people they have never met, doing things they cannot easily verify.

The conventional answer is "run more tests." Tests are good. Tests are not the answer to behavioral verification at supply-chain scale. Tests verify a finite point set; properties hold over the whole input domain. The tools that do verify properties (Kani, Prusti, F\*, Dafny, TLA+) are heavyweight, language-bound, and demand specifications written and maintained alongside the code. They do not federate. They do not compose across packages. They do not survive a `cargo update`.

Sugar is a toolchain for proving content-addressed claims. Behavioral
contracts are the first surface, but the same machinery proves claims about
protocol evolution, proof-file conformance, CI supply-chain input closure,
generated repair closure, package inspection, and materialization receipts. It
does not compete with the verification tools above. It sits beneath them and
turns their output into proofchains: portable, signed, content-addressed
evidence structures that compose across the dependency graph.

## The amortization move

A library publishes signed contract mementos along with its bytes. A consumer's
verifier loads the mementos, asks kits to resolve dependency `.proof` artifacts,
walks the obligations in the assembled system, and asks whether the graph
carries the needed implication.

When two canonical claims are byte-identical, their BLAKE3-512 CIDs match and
the obligation can discharge by equality. The verifier did not parse the
dependency, run the dependency, or invoke a solver for that identity case.

That is the cheap path, not the whole protocol. If a signed implication memento
already exists, the verifier can reuse that proof edge. If the graph does not
already carry the obligation, semantic proving still happens and the result can
be minted for future reuse. The lattice tractability theorem (CID
`blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07`)
is about this amortized content-addressed graph, not a promise that every new
proof is one comparison.

This is why the high-level primitive is a proofchain. The payload of a blockchain is a series of state transitions. The payload of a proofchain is a series of formal proofs. The implication is slight, but fundamental: the chain is carrying why a claim is logically true.

## Lift, don't author

The fundamental problem of formal verification has always been "how do we get the specifications?" For fifty years the answer was "convince developers to write them." That never worked at scale. Specifications are expensive, tedious, and live separately from the code, so they drift.

Sugar's answer is different: every annotation library in wide deployment already contains specifications. `proptest` invariants, `contracts` pre/post-conditions, `kani` proofs, `prusti` annotations, `pydantic` schemas, `zod` validators, `class-validator` decorators, `bean-validation` annotations, JML predicates, `go-playground/validator` tags. Each is an informal or semi-formal specification the codebase already maintains as part of normal development.

The lift adapter is the bridge. Per source library, an adapter walks the AST,
recognizes the library's idiom, and emits ProofIR or a protocol claim. The claim
is canonicalized, hashed, and wrapped in a signed memento. The signed memento is
a content-addressed unit of behavioral specification.

The codebase keeps its existing annotations. The author keeps their existing workflow. Sugar does not ask the author to learn a new spec language, write a parallel specification, or migrate to a different annotation library. The lift adapter promotes whatever the codebase already uses.

## Content-addressing, not a registry

Bitcoin demonstrated that a global ledger can exist with no central party. Git demonstrated that an entire software project's history can live in a content-addressed graph. BitTorrent demonstrated that the bytes of a petabyte file can flow without a server holding the master copy. IPFS demonstrated that "the address is the content" generalizes to arbitrary data.

Sugar is one more application of the same primitive, applied to behavioral verification. The "registry" is the BLAKE3-512 hashspace. Populated points are sparse: only the canonical-IR formulas that some kit has emitted exist as addresses. The protocol asks no central party's permission to mint new contracts, publish implications, or discover new mementos. The bytes verify themselves.

Cache invalidation is structurally absent. When a contract's bytes change, its
CID changes; old mementos remain cryptographically valid against the old bytes.
They do not silently apply to the new bytes. Provability is monotonic in that
old facts stay true about the content they named.

Search is grep. "Find a library function whose post-condition implies my pre-condition" is the npm-by-behavior query. The hashspace is a searchable substrate; the search reduces to walking `.proof` files and comparing CIDs.

## Three-tier handshake

Verification at scale is a cost-amortization problem. Naive per-call-site solver invocation does not scale. Sugar's handshake (CID `blake3-512:acbf67dda9373c648e591d8ad74b8f8d56f4c92ba9c82bdc6690dc521e6f17012dd195e98a96b099090eeeb5a424312d90ff441c882d0e317a190561aa1a6925`) breaks the cost model into three tiers, in increasing order of work:

1. **Hash equality.** The publisher's post-formula and the consumer's
   pre-formula canonicalize to identical bytes. The verifier is doing
   content-addressed lookup, not theorem proving.
2. **Cached implication.** A signed implication memento exists asserting
   `post -> pre`, witnessed by a prover and signed by a producer the verifier's
   policy admits. The verifier checks and reuses that edge.
3. **Solver fallback.** A prover is invoked for a genuinely novel `(post, pre)`
   pair. On success, the result can be minted as a new signed implication
   memento so future verifiers can reuse it.

The headline metric is the hash-discharge fraction: the share of call sites discharged by Tier 1 alone. A high fraction means contracts are composing well across the ecosystem. The implication server (a passive indexer, not an authority) reports observed coverage.

## Compile-time errors, not runtime probes

Build-script integration lifts contract violations into compile-time errors. Sugar becomes a smarter type system extension: a violated handshake fails the build, not a test run. The proof gate is the same gate the compiler already enforces, extended with semantic claims the type system cannot represent.

This is what the term "constraint-driven development" names. Software ages backwards: as the lattice of published implications grows, every codebase that adopts Sugar becomes easier to verify than the one shipped yesterday. The framework's value compounds with adoption rather than degrading.

## Install path

```bash
cargo install --path implementations/rust/provekit-cli
provekit verify-protocol
cd your-rust-crate
cargo provekit-lift
provekit prove
```

The Rust CLI is the canonical shipping implementation. Per-language libs embed
or call the verifier; per-language kits emit canonical IR and extension bodies;
per-language lift adapters bridge from existing annotation libraries. See
[docs/reference/per-language-status.md](../reference/per-language-status.md) for
the matrix.

## What's not in the box

Sugar is honest about scope. It is not a soundness-certified compliance tool; if a regulator requires Coq, Isabelle, or F\* output, those tools remain the right choice. It is not a substitute for runtime testing; properties hold over the input domain only insofar as the lifted IR faithfully encodes the source library's idiom, and per-library adapter coverage is empirical. It is not a substitute for human review; signed mementos document who claimed what, but the trust decision belongs to the verifier.

What Sugar is, is the load-bearing primitive missing from the verification
ecosystem: proofchains over which existing tools can publish their findings in a
portable, signed, composable form. The lift-not-author posture is the adoption
story. Content-addressed reuse is the scaling story.

## Read further

- [README.md](../../README.md) for the install path.
- [proofchain.md](proofchain.md) for the high-level primitive.
- [../papers/03-substrate-not-blockchain.md](../papers/03-substrate-not-blockchain.md) for the consensus lemma.
- [architecture.md](architecture.md) for the four-layer model and handshake.
- [thesis.md](thesis.md) for the deeper architectural claim.
- [protocol/specs/](../../protocol/specs/) for the canonical specs, addressed by CID.
