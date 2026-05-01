# ProvekIt: verify a petabyte of behavior by comparing 64 bytes

A modern Rust workspace pulls in a few hundred crates. A modern npm project pulls in tens of thousands. A modern Go service drags in transitive C dependencies through cgo. Behind every `cargo build`, `npm install`, and `go mod tidy`, a developer is implicitly trusting a graph of code they have never read, written by people they have never met, doing things they cannot easily verify.

The conventional answer is "run more tests." Tests are good. Tests are not the answer to behavioral verification at supply-chain scale. Tests verify a finite point set; properties hold over the whole input domain. The tools that do verify properties (Kani, Prusti, F\*, Dafny, TLA+) are heavyweight, language-bound, and demand specifications written and maintained alongside the code. They do not federate. They do not compose across packages. They do not survive a `cargo update`.

ProvekIt is a content-addressed protocol for behavioral contracts. It does not compete with the verification tools above. It sits beneath them and makes their output portable, signed, and composable across the dependency graph.

## The petabyte-to-64-bytes ratio

A library publishes signed contract mementos along with its bytes. A consumer's verifier loads the mementos, walks the call sites in the consumer's own code, and asks one question per call site: does the publisher's post-condition imply the consumer's pre-condition? After canonicalization to a deterministic IR, the answer reduces to comparing two BLAKE3-512 hashes. Sixty-four bytes versus sixty-four bytes. One CPU instruction.

When the hashes match, the call site is discharged. The verifier did not parse the dependency. It did not run the dependency. It did not invoke a solver. It compared 64 bytes.

This is the protocol's headline arithmetic. Verification of an arbitrarily-deep dependency stack reduces to hash comparison; the verifier's cost is decoupled from the size of the stack. The lattice tractability theorem (CID `blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07`) makes the claim formal: honest verifier cost is a function of grammar parameters and decision-procedure complexity, not of the populated cardinality of the address space and not of the cryptographic security parameter. The 2^512 hashspace is a property of the address space, not the search space.

## Lift, don't author

The fundamental problem of formal verification has always been "how do we get the specifications?" For fifty years the answer was "convince developers to write them." That never worked at scale. Specifications are expensive, tedious, and live separately from the code, so they drift.

ProvekIt's answer is different: every annotation library in wide deployment already contains specifications. `proptest` invariants, `contracts` pre/post-conditions, `kani` proofs, `prusti` annotations, `pydantic` schemas, `zod` validators, `class-validator` decorators, `bean-validation` annotations, JML predicates, `go-playground/validator` tags. Each is an informal or semi-formal specification the codebase already maintains as part of normal development.

The lift adapter is the bridge. Per source library, an adapter walks the AST, recognizes the library's idiom, and emits a canonical IR. The IR is canonicalized, hashed, and wrapped in a contract memento envelope. The envelope is signed. The signed memento is a content-addressed unit of behavioral specification.

The codebase keeps its existing annotations. The author keeps their existing workflow. ProvekIt does not ask the author to learn a new spec language, write a parallel specification, or migrate to a different annotation library. The lift adapter promotes whatever the codebase already uses.

## Content-addressing, not a registry

Bitcoin demonstrated that a global ledger can exist with no central party. Git demonstrated that an entire software project's history can live in a content-addressed graph. BitTorrent demonstrated that the bytes of a petabyte file can flow without a server holding the master copy. IPFS demonstrated that "the address is the content" generalizes to arbitrary data.

ProvekIt is one more application of the same primitive, applied to behavioral verification. The "registry" is the BLAKE3-512 hashspace. Populated points are sparse: only the canonical-IR formulas that some kit has emitted exist as addresses. The protocol asks no central party's permission to mint new contracts, publish implications, or discover new mementos. The bytes verify themselves.

Cache invalidation is structurally absent. When a contract's bytes change, its CID changes; old mementos remain cryptographically valid against the old bytes; nothing is poisoned, nothing is stale, nothing requires a service call to refresh. Provability is monotonic. A fact, once minted, is a hash lookup forever.

Search is grep. "Find a library function whose post-condition implies my pre-condition" is the npm-by-behavior query. The hashspace is a searchable substrate; the search reduces to walking `.proof` files and comparing CIDs.

## Three-tier handshake

Verification at scale is a cost-amortization problem. Naive per-call-site solver invocation does not scale. ProvekIt's handshake (CID `blake3-512:acbf67dda9373c648e591d8ad74b8f8d56f4c92ba9c82bdc6690dc521e6f17012dd195e98a96b099090eeeb5a424312d90ff441c882d0e317a190561aa1a6925`) breaks the cost model into three tiers, in increasing order of work:

1. **Hash equality.** The publisher's post-formula and the consumer's pre-formula canonicalize to identical bytes. `memcmp` is zero. The call site is discharged for free. This is the librarian, not the expert: the verifier is doing content-addressed lookup, not theorem proving.
2. **Cached implication.** A signed implication memento exists asserting `post → pre`, witnessed by Z3, signed by some prover. The verifier checks the signature once per `(post, pre)` pair. Constant time, branch-free, network-free.
3. **Solver fallback.** Z3 is invoked once per genuinely-novel `(post, pre)` pair. On `unsat`, the result is minted as a new signed implication memento and published per the verifier's policy. Every future verifier in the ecosystem hits Tier 2 instead.

The headline metric is the hash-discharge fraction: the share of call sites discharged by Tier 1 alone. A high fraction means contracts are composing well across the ecosystem. The implication server (a passive indexer, not an authority) reports observed coverage.

## Compile-time errors, not runtime probes

The Rust build-script integration (in flight, planned for v1.2) lifts contract violations into compile-time errors. ProvekIt becomes a smarter type system extension: a violated handshake fails the build, not a test run. The proof gate is the same gate the compiler already enforces, extended with semantic claims the type system cannot represent.

This is what the term "constraint-driven development" names. Software ages backwards: as the lattice of published implications grows, every codebase that adopts ProvekIt becomes easier to verify than the one shipped yesterday. The framework's value compounds with adoption rather than degrading.

## Install path

```bash
cargo install provekit
provekit verify-protocol
cd your-rust-crate
cargo provekit-lift
provekit prove
```

The Rust CLI is the canonical shipping implementation for v1.1.0. Per-language libs (TypeScript, Go, C++) embed the verifier; per-language kits (authoring) emit canonical IR; per-language lift adapters bridge from existing annotation libraries. See `docs/per-language-status.md` for the matrix.

## What's not in the box

ProvekIt is honest about scope. It is not a soundness-certified compliance tool; if a regulator requires Coq, Isabelle, or F\* output, those tools remain the right choice. It is not a substitute for runtime testing; properties hold over the input domain only insofar as the lifted IR faithfully encodes the source library's idiom, and per-library adapter coverage is empirical. It is not a substitute for human review; signed mementos document who claimed what, but the trust decision belongs to the verifier.

What ProvekIt is, is the load-bearing primitive missing from the verification ecosystem: a content-addressed substrate over which the existing tools can publish their findings in a portable, signed, composable form. The petabyte/64-byte ratio is the headline. The lift-not-author posture is the adoption story. The Bitcoin/Git/BitTorrent lineage is the reason it scales.

## Read further

- [README.md](README.md) for the install path.
- [ARCHITECTURE.md](ARCHITECTURE.md) for the four-layer model and handshake.
- [THESIS.md](THESIS.md) for the deeper architectural claim.
- [protocol/specs/](protocol/specs/) for the canonical specs, addressed by CID.
