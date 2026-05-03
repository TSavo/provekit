# ProvekIt papers

This directory contains paper-grade arguments. These are not docs (how to use the protocol) and not specs (the formal definition of the protocol). They are sustained arguments about what the protocol *is*, why its design choices are load-bearing, and what its consequences are.

Each paper is intended to be cite-able in academic and industry contexts.

## Index

The papers compose. Read them in order; later papers assume the framing earlier ones established.

1. **[The Whitepaper](01-whitepaper.md)** — Executive summary. What ProvekIt is, why it matters, who it is for, the trojan horse, the cypherpunk lineage. The trust-depth knob; the three-CLI surface; the install path. The shortest path from "I have heard of this" to "I understand the move."

2. **[The Bluepaper](02-bluepaper.md)** — Formal protocol specification. Theorem statements with proofs; the canonical IR grammar in EBNF; the constant-size verification theorem with hypotheses H1-H5; the `memcmp` line shown verbatim; the cryptographic-minimum claim; every spec referenced by content hash. Closes with a runnable verification: compute the catalog CID locally, and the bluepaper has just verified its own authority. Pinned at v1.4.0 catalog CID.

3. **[Substrate, not Blockchain](03-substrate-not-blockchain.md)** — The manifesto. §10 closure-by-composition, §11 the address as multi-dimensional, §12 the pin as a tuple. The four pinning dimensions (`contractCid` / `contractSetCid` / `attestationCid` / bundle file CID), each a different content projection. The rank-N tuple discipline: a pin must match the rank of the assertion it attests; collapsing a rank-2 relation into rank-1 leaks the discarded axis as drift. This is the architectural foundation the v1.4 specs operationalize.

4. **[The Vertical Stack, and the Road to Standardization](04-vertical-stack-and-standardization.md)** — A `.proof` and a chain of formally verified software from quantum physics to bytecode are 1:1 identical at the data-structure level. The standardization roadmap through DO-178C, Common Criteria EAL5+, ISO 26262, FDA SaMD, FedRAMP, IEC 62304, NIST SSDF, SLSA, EU Cyber Resilience Act. 5/10/15/20-year horizons.

## Future papers (planned)

- *Multi-dimensional pinning as supply-chain integrity*: rank-3 pins close the lying-contract attack class that single-CID pinning leaves open. Why v1.4's address-space-as-vector-space framing is the substrate the standards-track work needs.
- *On the lattice tractability theorem and its consequences*: a deeper exposition of the cost model.
- *The lift-not-author posture: a fifty-year-overdue reframe*: why specifications have always been there, and what changes when we lift them.
- *Cross-domain verification by content-addressing*: the bridge mechanism in mathematical detail.
- *Trust as a local decision: post-central-authority verification*: the trust model and its implications for governance.

## Contributing a paper

A paper-grade contribution argues a sustained position with engagement of counterarguments, references, and operational consequences. If you have such an argument, file a draft in `docs/papers/draft-NN-<topic>.md` and request review.

Papers are not technical reports; they engage the larger conceptual landscape. They are not blog posts; they engage counterarguments seriously. Both are valuable elsewhere; this directory is for the load-bearing arguments specifically.
