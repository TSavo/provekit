# Boundaries: what ProvekIt is NOT

Every claim about what a thing does is implicitly a claim about what it doesn't. This document is the explicit non-claim list. If you came expecting any of these, you came expecting wrong.

The thesis is unusually load-bearing. Stripping it of overclaims makes it stronger, not weaker.

## The single sentence

**ProvekIt is not a formal verification framework. ProvekIt is a protocol for content-addressing formal verifications.**

That distinction is the whole document, restated below in a dozen forms with worked examples. Frameworks like Coq, F\*, Lean, Isabelle, Kani, Prusti, Creusot, Dafny, TLA+, CBMC are formal verification frameworks. They produce verifications. ProvekIt does not produce verifications; ProvekIt is the substrate over which verifications are published, signed, distributed, federated, and composed.

Same primitive as Bitcoin (content-addressed currency), Git (content-addressed source history), BitTorrent (content-addressed file distribution), IPFS (content-addressed web). Each of those took a domain that was thought to require a central authority and showed it admits a content-addressed protocol with no central party. ProvekIt is one more application of the same primitive, applied to behavioral verification.

If you mistake the protocol for a framework, every other not-claim in this document follows from the misreading. Read the not-claims as ways of saying the same single thing.

## ProvekIt is not a substitute for testing

Tests verify behavior on a finite point set: the inputs you wrote. Properties hold over an input domain. ProvekIt addresses the second; it does not replace the first.

A typical project ships unit tests, integration tests, end-to-end tests, performance tests, regression tests, and a `.proof` file. The `.proof` is one more piece of the verification stack. Removing tests because "we have proofs now" is a misuse.

Specifically: tests catch what proofs don't (timing-dependent failures, real I/O, real network, real concurrency, environment-specific issues, integration with external services). Proofs catch what tests don't (the input you didn't write, the dependency interaction you didn't predict, the cross-language call site you didn't audit).

Use both. Reject any framing that pits them against each other.

## ProvekIt is not soundness-certified

A `provekit prove` "discharged" result is not a Coq-style certificate. The protocol's correctness rests on:

1. **Cryptographic assumptions.** BLAKE3-512 is collision-resistant, Ed25519 is unforgeable. Both are widely-deployed primitives but neither is unconditionally proven secure.
2. **Solver correctness.** Tier 3 invokes Z3 (or another configured backend). Z3 has had bugs. A `unsat` from Z3 is trusted to be sound; if Z3 is wrong, ProvekIt's discharge is wrong.
3. **Per-adapter faithfulness.** Lift adapters map source-library annotations to canonical IR. If an adapter mis-translates an annotation, the canonical IR doesn't reflect what the annotation actually means.
4. **Canonicalization correctness.** JCS encoding has implementation bugs in the wild. If your kit's JCS implementation drifts from the spec, your hashes are wrong.
5. **Kit conformance.** Conformance fixtures cover the cases the fixtures cover. Out-of-fixture behavior is not verified.

Each of these can fail. None is unconditional. ProvekIt does not produce a proof that all five are correct simultaneously. It produces a portable artifact that, conditional on those assumptions, asserts a behavioral property.

If a regulator requires Coq, F\*, Lean, or Isabelle output, those tools remain the right choice. ProvekIt is not them.

## ProvekIt is not a substitute for human review

The signature on a `.proof` documents who claimed what. It does not vouch for the claimant's competence, judgment, or motivation. A signed memento from a malicious developer is a valid memento at the signature layer; the verifier checks the trust set, not the claimant's intent.

Under v1.4's rank-3 consumer pin (`contractCid, witnessCid, binaryCid`), the consumer's threat model is rich: they choose which contractCids they accept, which prover keys they trust for witnesses, and they verify the binaryCid against the running artifact. A malicious developer would need to (a) author a contract the consumer is willing to bind to, AND (b) procure a witness signed by a prover the consumer trusts, AND (c) ship a binary whose hash matches what the consumer verifies — coherently. That bar is high; not infinite.

The trust decision belongs to the verifier:

- Whose signing keys are trusted?
- What's the signing key's revocation status?
- Has the signer ever shipped wrong contracts? With what consequences?
- Is the codebase under threat of insider risk?

ProvekIt provides the substrate for these decisions. The decisions remain human.

## ProvekIt is not a runtime monitor

The `.proof` is checked at build time (or when the verifier explicitly runs). The protocol does not run in the user's program at runtime, does not intercept calls, does not wrap functions, does not produce runtime checks.

Adapters that implement runtime checking (e.g., the `@provekit.contract` decorator in Python optionally wraps functions with runtime predicate evaluation) do that as a separate, opt-in layer. The protocol layer is purely static.

This is a feature: ProvekIt's overhead at runtime is zero. The cost is paid at build time, once per `(post, pre)` pair, amortized across every consumer who hits the same pair via cached implications.

## ProvekIt does not turn empirical software into mathematical software

Most software is empirical. It works because it has been tested, used, observed, debugged. The proof of correctness is the absence of counterexamples in the wild.

ProvekIt does not turn this empirical software into mathematical software. It turns one specific class of behavioral verification (the kind expressible in the IR's logical fragments) into a content-addressed substrate that composes across the dependency graph.

The remainder of the codebase remains empirical. Most of `parseInt` is "we've shipped this for years and the bug count is low." A small but significant slice ("input validation, type coercion, return type guarantees") is now hash-bounded across implementations. The rest is still empirical software.

This is what "the residue at Tier 3 is real work" means. The IR's expressiveness is the boundary; what falls outside the IR is empirical until proven otherwise.

## ProvekIt does not solve the cold-start problem

The headline metric — hash-discharge fraction — depends on the lattice being seeded with enough cached implications to make Tier 1 the common case. An empty lattice means every call site falls through to Tier 3 (Z3 invocation per pair), which is slow.

Early adopters of ProvekIt face an empty lattice. Their first runs are mostly Tier 3, mostly slow, with low Tier 1 fraction. The lattice grows from each project that adopts; the cost amortizes; the discharge fraction approaches the theoretical asymptote.

ProvekIt does not magically populate the lattice. See [`cold-start.md`](cold-start.md) for the bootstrap discussion.

## ProvekIt does not have zero adoption cost

The lift-not-author posture minimizes adoption cost: the codebase keeps its existing annotations, the developer keeps their workflow. But "minimized" is not "zero."

Real adoption costs:

- **Per-source-library lift adapter authoring.** Each annotation library needs an adapter. Today: about 15 ship; tens more on the roadmap.
- **Per-language kit authoring.** Each host language needs a kit. Today: 11 ship at varying coverage levels.
- **Per-prover-backend authoring.** Tier 3 backends. Today: Z3 only. Adding Lean, CBMC, etc. is real engineering.
- **Reference contracts curation.** The bridge anchors that make cross-domain transfer work. Today: a small set; growing.
- **CI / IDE / build-script integration.** Per-toolchain glue.

For a single team adopting ProvekIt in a single Rust workspace, adoption is one `cargo install` and a few `cargo provekit-lift` invocations. For a polyglot company adopting across all their codebases, adoption is months of engineering plus standing up internal reference contracts.

## ProvekIt does not eliminate the need for Z3 (or any solver)

Tier 3 of the handshake exists because some `(post, pre)` pairs are genuinely novel and require a solver. The protocol doesn't pretend solvers are unnecessary; it amortizes their cost.

A claim that "ProvekIt replaces the solver" misreads the architecture. ProvekIt invokes the solver, captures the result, distributes the result, and ensures every future verifier reuses the result. The solver still runs.

The replacement claim against solvers is for *redundant* solver invocations across the dependency graph. The solver runs once per genuinely-new pair, not once per call site.

## ProvekIt does not provide a correctness audit

A `.proof` describes what a developer asserted, not what is true. If a developer signs a contract memento that mis-states their function's behavior, the signature is valid; the assertion is wrong; the verifier discharges call sites against the wrong contract.

This is a non-trivial threat model:

- **Honest mistake.** Developer signs a contract that doesn't match their function. The function does the right thing; the contract describes the wrong thing. Consumers verifying against the contract get a false sense of confidence.
- **Adversarial misrepresentation.** Attacker writes a function that does X, signs a contract claiming the function does Y. Consumers trust the contract; the function does X; consumers are exploited.

ProvekIt does not detect honest mistakes; it does not detect adversarial misrepresentation. The signature attests to the *signer*; it does not attest to *correctness*.

What ProvekIt provides is non-repudiation (the signer cannot deny having signed the contract) and tamper-evidence (the contract cannot be altered after signing without breaking the signature). These are useful, but they are not a correctness audit.

A correctness audit requires either:

1. Re-running the solver on the contract under your own threat model (and your own choice of trusted solver).
2. Inspecting the binary (whose CID is pinned via `binaryCid`).
3. Trusting the signer's reputation, organization, or audit trail.

ProvekIt makes (1) cheap and (2) verifiable but does not perform either for you.

## ProvekIt is not Bitcoin

The lineage to Bitcoin / Git / BitTorrent / IPFS is structural — content-addressing as a primitive. The lineage is not financial, not adversarial-minded in the same way, not subject to the same incentive engineering.

Specifically: ProvekIt does not have a token, does not have miners, does not have a consensus mechanism, does not have block reorganizations. The "registry" is a passive set of mementos that any peer can store; there is no party whose downtime breaks anything; there is no adversarial validation game.

Comparisons to Bitcoin's *primitives* (content-addressing, signed memento publication, no central authority) are useful. Comparisons to Bitcoin's *economics* (mining, scarcity, governance) are not.

## What you do get

After all the not-claims:

- Behavioral contracts as content-addressed signed mementos.
- A 64-byte hash comparison as the common-case verification.
- Cross-language proof transfer via shared reference contracts.
- A monotonic lattice that compounds with adoption.
- A protocol whose CID is its own version, whose specs are content-addressed, whose conformance is empirically checkable.

That is the thesis. Stripping the overclaims makes the remaining claim clearer.

## Read next

- [thesis.md](thesis.md) — the central claim, in detail.
- [cold-start.md](cold-start.md) — the bootstrap problem.
- [../security/threat-model.md](../security/threat-model.md) (when written) — what ProvekIt catches and what it doesn't.
- [compared-to/](compared-to/) — head-to-head comparisons with adjacent tools.
