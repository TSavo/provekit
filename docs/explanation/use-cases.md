# Sugar Use Cases

Sugar is a toolchain for proving content-addressed claims. A claim can be about source behavior, protocol evolution, a proof-file parser, a generated transform, or a supply-chain artifact. The shape stays the same:

1. identify the proposition;
2. canonicalize it;
3. name the proposition and evidence by CID;
4. sign the edge;
5. fail closed unless the graph carries the claim.

## Software Correctness Across Domains

This is the most visible use case. A Rust invariant, a Java Bean Validation annotation, a Spring boundary, a TypeScript Zod validator, and a Go struct tag can express the same boundary fact. Sugar lifts those surfaces to ProofIR, hashes the canonical proposition, and lets a verifier compare claims across languages and packages.

The important point is not "cross-language" by itself. The point is domain crossing: source idiom to IR, library contract to consumer precondition, package proof to application proof, and one implementation's claim to another implementation's claim.

## Supply-Chain Integrity

The `.proof` bundle is a signed, content-addressed statement about what an artifact claims, which bytes those claims bind to, and which witnesses support them. Ranked pins such as `(contractCid, witnessCid, binaryCid)` let consumers distinguish contract content, evidence, and compiled artifact bytes instead of collapsing them into one honor-system label.

## Protocol Evolution

PEP treats a protocol change as data. The transition from one catalog CID to another is represented by a `ProtocolEvolutionBodyClaim`, a catalog diff, policy, verifier identity, signed catalog attestation, and a TDP-shaped witness.

This keeps protocol governance inside the same substrate as everything else. Core verification does not execute PEP. Core verification checks signed bytes, CIDs, and references. Extension-aware tooling may admit the evolution edge under policy.

## Proof-File Conformance

The `.proof` format is itself a protocol surface. Conformance belongs behind the same current proof/verification gates as other substrate checks; the Rust CLI no longer exposes a separate manual `.proof` file inspection command.

This is Sugar proving a parser/consumer claim rather than an application contract.

## Bug Rediscovery And Closure

Bug Zoo specimens are small, executable examples of latent boundary failures. A specimen starts in `lab/`, where ordinary host checks pass and no Sugar workflow is configured. It moves to `exhibit/`, where one or more surfaces lift the missing edge into ProofIR or link it into a cross-kit LinkBundle and produce the red CLI signal. The paired `fixed/` surfaces are accepted only when the same path re-runs to a green boundary receipt.

The historical patch is not the oracle. The durable claim is independent rediscovery and independent closure: Sugar found the missing `p => q` edge and accepted closure only after re-lift or re-link proved the boundary was closed.

## Realizers And Droppers

ORP names the family of components that realize missing obligations. A witnesser observes and emits evidence. A dropper mutates or emits a host artifact candidate. A monitor installs a runtime witness stream.

Droppers are never trusted by origin. The transform is a candidate until the resulting host artifact is re-lifted and the closure witness is present. That is why a realizer output carries CIDs for the source artifact, transformed artifact, post-lift document, closure witness, proof plan, language projection, and fix receipt.

## Checker Bytecode And Grammar Conformance

CBP covers executable checker artifacts for ProofIR obligations. GCP covers claims that a body conforms to a formal grammar and optional ProofIR invariant set. Both are extension protocols: core verification validates the signed graph, while policy-aware tooling decides whether to execute parsers, invariant checkers, or bytecode.

These protocols matter because Sugar can prove its own extension surfaces without making the core verifier circular.
