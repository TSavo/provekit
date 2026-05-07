# ProvekIt papers

This directory contains paper-grade arguments. These are not docs (how to use the protocol) and not specs (the formal definition of the protocol). They are sustained arguments about what the protocol *is*, why its design choices are load-bearing, and what its consequences are.

Each paper is intended to be cite-able in academic and industry contexts.

## Index

The papers compose. Read them in order; later papers assume the framing earlier ones established.

1. **[The Whitepaper](01-whitepaper.md)**: Executive summary. What ProvekIt is, why it matters, who it is for, the trojan horse, the cypherpunk lineage. The trust-depth knob; the three-CLI surface; the install path. The shortest path from "I have heard of this" to "I understand the move."

2. **[The Bluepaper](02-bluepaper.md)**: Formal protocol specification. Theorem statements with proofs; the canonical IR grammar in EBNF; the constant-size verification theorem with hypotheses H1-H5; the `memcmp` line shown verbatim; the cryptographic-minimum claim; every spec referenced by content hash. Closes with a runnable verification: compute the catalog CID locally, and the bluepaper has just verified its own authority. Pinned at v1.4.0 catalog CID.

3. **[Substrate, not Blockchain](03-substrate-not-blockchain.md)**: The manifesto. §10 closure-by-composition, §11 the address as multi-dimensional, §12 the pin as a tuple. The four pinning dimensions (`contractCid` / `contractSetCid` / `attestationCid` / bundle file CID), each a different content projection. The rank-N tuple discipline: a pin must match the rank of the assertion it attests; collapsing a rank-2 relation into rank-1 leaks the discarded axis as drift. This is the architectural foundation the v1.4 specs operationalize.

4. **[The Vertical Stack, and the Road to Standardization](04-vertical-stack-and-standardization.md)**: A `.proof` and a chain of formally verified software from quantum physics to bytecode are 1:1 identical at the data-structure level. The standardization roadmap through DO-178C, Common Criteria EAL5+, ISO 26262, FDA SaMD, FedRAMP, IEC 62304, NIST SSDF, SLSA, EU Cyber Resilience Act. 5/10/15/20-year horizons.

5. **[Witness Pluralism and Jurisdiction-Neutral Transport](05-witness-pluralism-and-jurisdiction-neutral-transport.md)**: Why the substrate moves through any pipe (HTTP, IPFS, dead-drops) and why federation across jurisdictions doesn't require any single party. The transport layer's design as an explicit non-feature, and what that buys.

6. **[After Reputation: Software as Federated Truth-Claims](06-after-reputation-software-as-federated-truth-claims.md)**: The consequence of shipping the protocol. Why the substrate replaces reputation as the load-bearing trust mechanism in supply chains, engineering practice, liability, and the closed/open dichotomy. The substrate is the diplomatic protocol between every truth-claim about software ever made.

7. **[After Verification: Bug Classes as Missing Edges in the Federated Proof Substrate](07-after-verification-bug-classes-as-missing-edges.md)**: The deeper consequence. Once droppers close the loop with lifters over weakest-precondition propagation, leaf-discharge bug classes become structurally impossible. Contains a constructive theorem (Structural Elimination of Leaf-Discharge Bug Classes) with proof by induction on data-flow path length. Articulates the substrate's algebraic shape (thin Heyting category over content-addressed predicates), the completeness lemma (`Allocations × Reads` is enumerable and exhaustive), and the generative-completion property (the substrate computes what is missing and writes the code that supplies it). Cousot 1977 plus content-addressing plus federation, lifted to a planet-scale proof DAG.

8. **[After Types: How I Learned to Stop Logging and Trust the Invariant Solver](08-after-types-stop-logging-trust-the-invariant-solver.md)**: The developer-workflow consequence. Types and forensic logs survive, but lose their load-bearing correctness role once invariant proof is a federated substrate. Types become editorial scaffolding; logs remain operational observability. The wall against leaf-discharge bugs moves to content-addressed invariant edges.

9. **[Lossy Boundary Compression: Why ProofIR Is Universal Because It Forgets](09-lossy-boundary-compression.md)**: The universality argument for ProofIR. ProofIR is not a universal language for re-expressing every implementation detail; it is universal over contract boundaries. Lifters may discard implementation texture while preserving obligations, making cross-language, cross-framework, cross-time equivalence possible and turning LLM output into an admissibility-search problem.

## Future papers (planned)

- *Multi-dimensional pinning as supply-chain integrity*: rank-3 pins close the lying-contract attack class that single-CID pinning leaves open. Why v1.4's address-space-as-vector-space framing is the substrate the standards-track work needs.
- *On the lattice tractability theorem and its consequences*: a deeper exposition of the cost model.
- *The lift-not-author posture: a fifty-year-overdue reframe*: why specifications have always been there, and what changes when we lift them.
- *Cross-domain verification by content-addressing*: the bridge mechanism in mathematical detail.
- *Trust as a local decision: post-central-authority verification*: the trust model and its implications for governance.

## Contributing a paper

A paper-grade contribution argues a sustained position with engagement of counterarguments, references, and operational consequences. If you have such an argument, file a draft in `docs/papers/draft-NN-<topic>.md` and request review.

Papers are not technical reports; they engage the larger conceptual landscape. They are not blog posts; they engage counterarguments seriously. Both are valuable elsewhere; this directory is for the load-bearing arguments specifically.
