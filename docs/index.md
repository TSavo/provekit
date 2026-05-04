# ProvekIt documentation

**ProvekIt is not a formal verification framework. It is a protocol for content-addressing formal verifications.**

Same primitive as Bitcoin (currency), Git (source history), BitTorrent (content distribution), IPFS (the addressable web) — applied to behavioral verification. Verify a petabyte of behavior by comparing 64 bytes.

This page is a routing front door. Pick the row that matches what brought you here.

## I want to...

### Use ProvekIt in my code

| If you write... | Start here | Status |
|---|---|---|
| **Rust** | [tutorials/rust.md](tutorials/rust.md) | shipping (canonical) |
| **TypeScript** | [tutorials/typescript.md](tutorials/typescript.md) | kit + 3 lift adapters shipping; verify via Rust CLI |
| **Python** | [tutorials/python.md](tutorials/python.md) | kit + pydantic shipping; verify via Rust CLI |
| **Java / JVM** | [tutorials/java.md](tutorials/java.md) | kit + 4 lift adapters shipping; verify via Rust CLI |
| **C#** | [tutorials/csharp.md](tutorials/csharp.md) | kit + DataAnnotations + LINQ shipping; verify via Rust CLI |
| **Ruby** | [tutorials/ruby.md](tutorials/ruby.md) | kit + 3 lift adapters shipping; verify via Rust CLI |
| **Zig** | [tutorials/zig.md](tutorials/zig.md) | kit + comment-based lift shipping; verify via Rust CLI |
| **Go** | [tutorials/go.md](tutorials/go.md) | kit shipping; lift adapters v1.2 |
| **C++** | [tutorials/cpp.md](tutorials/cpp.md) | kit shipping; C++26 contracts lift v1.2 |
| **Swift** | [tutorials/swift.md](tutorials/swift.md) | kit shipping (PR #76); lift planned |
| **C** | [tutorials/c.md](tutorials/c.md) | kit shipping; lift planned |
| **A polyglot stack** (multiple of the above) | [tutorials/polyglot-stack.md](tutorials/polyglot-stack.md) | the cross-domain demo |

### Wire it into my workflow

| Goal | Start here |
|---|---|
| Get red squigglies in my IDE | [how-to/ide-integration/](how-to/ide-integration/) |
| Block my CI on proof failure | [how-to/ci-integration/](how-to/ci-integration/) |
| Ship a `.proof` alongside my package | [how-to/publishing-a-proof.md](how-to/publishing-a-proof.md) |
| Verify a dependency's `.proof` | [how-to/consuming-a-proof.md](how-to/consuming-a-proof.md) |
| Pin the compiled binary as a supply-chain anchor | [how-to/pinning-a-binary.md](how-to/pinning-a-binary.md) |
| Author a contract directly (no lift target) | [how-to/authoring-contracts.md](how-to/authoring-contracts.md) |
| Bind an implementation to a reference contract | [how-to/cross-domain-bridges.md](how-to/cross-domain-bridges.md) |
| Debug a failed handshake | [how-to/debugging-a-failed-handshake.md](how-to/debugging-a-failed-handshake.md) |
| Manage signing keys | [how-to/managing-keys.md](how-to/managing-keys.md) |
| Adopt ProvekIt gradually in an existing codebase | [how-to/migrating-to-provekit.md](how-to/migrating-to-provekit.md) |

### Sustained arguments (papers)

| Paper | Argument |
|---|---|
| [The vertical stack, and the road to standardization](papers/01-vertical-stack-and-standardization.md) | A `.proof` and a chain of formally verified software from quantum physics to bytecode are 1:1 identical at the data-structure level. Standardization roadmap through DO-178C, Common Criteria, ISO 26262, FDA, FedRAMP, EU CRA. |

See [papers/](papers/) for the index.

### Understand the protocol

| Question | Read |
|---|---|
| What's the central claim? | [explanation/thesis.md](explanation/thesis.md) |
| How does it work, end to end? | [explanation/architecture.md](explanation/architecture.md) |
| What does it replace and complement? | [explanation/product.md](explanation/product.md) |
| Why "lift, don't author"? | [explanation/lift-not-author.md](explanation/lift-not-author.md) |
| Why no central registry? | [explanation/content-addressing-not-registry.md](explanation/content-addressing-not-registry.md) |
| Why is provability monotonic? | [explanation/monotonic-provability.md](explanation/monotonic-provability.md) |
| What's the cold-start story? | [explanation/cold-start.md](explanation/cold-start.md) |
| What is ProvekIt **not**? | [explanation/boundaries.md](explanation/boundaries.md) |
| How does it compare to Coq / F\* / Lean? | [explanation/compared-to/coq-fstar-lean.md](explanation/compared-to/coq-fstar-lean.md) |
| How does it compare to Kani / Prusti / Creusot? | [explanation/compared-to/kani-prusti-creusot.md](explanation/compared-to/kani-prusti-creusot.md) |
| How does it compare to SLSA / Sigstore / SCITT / SBOM formats? | [explanation/compared-to/slsa-sigstore-in-toto-scitt.md](explanation/compared-to/slsa-sigstore-in-toto-scitt.md) |

### Look something up

| Looking for... | Go to |
|---|---|
| CLI subcommand reference | [reference/cli/](reference/cli/) |
| IR grammar (CDDL) | [reference/ir/grammar.md](reference/ir/grammar.md) |
| `.proof` bundle format | [reference/proof-bundle/format.md](reference/proof-bundle/format.md) |
| Handshake algorithm | [reference/handshake/algorithm.md](reference/handshake/algorithm.md) |
| Per-language status matrix | [reference/per-language-status.md](reference/per-language-status.md) |
| Per-source-library lift coverage | [reference/per-adapter-coverage.md](reference/per-adapter-coverage.md) |
| Kit standard (every kit must implement) | [reference/kit-standard.md](reference/kit-standard.md) |
| LSP plugin protocol | [reference/lsp-plugin-protocol.md](reference/lsp-plugin-protocol.md) |
| Error codes | [reference/error-codes.md](reference/error-codes.md) |
| Spec CIDs at HEAD | [reference/cids.md](reference/cids.md) |
| Lattice tractability theorem | [reference/lattice-tractability.md](reference/lattice-tractability.md) |
| Glossary (CID, memento, bridge, kit, adapter, lift, handshake, tier, sort, declaration) | [glossary.md](glossary.md) |

### Contribute

| Goal | Start here |
|---|---|
| Add support for a new host language | [contributing/porting-to-a-new-language.md](contributing/porting-to-a-new-language.md) |
| Write a kit (canonicalizer, envelopes, self-contracts) | [contributing/writing-a-kit/](contributing/writing-a-kit/) |
| Write a lift adapter for an annotation library | [contributing/writing-a-lift-adapter/](contributing/writing-a-lift-adapter/) |
| Write an LSP plugin | [contributing/writing-an-LSP-plugin.md](contributing/writing-an-LSP-plugin.md) |
| Write a prover backend (Lean, TLA+, CBMC) | [contributing/writing-a-prover-backend.md](contributing/writing-a-prover-backend.md) |
| Propose a spec change | [contributing/proposing-a-spec-change.md](contributing/proposing-a-spec-change.md) |
| Cut a release | [contributing/release-process.md](contributing/release-process.md) |

### Operate it

| Goal | Start here |
|---|---|
| Run an implication server | [operations/running-an-implication-server.md](operations/running-an-implication-server.md) |
| Monitor discharge fraction over time | [operations/monitoring-and-metrics.md](operations/monitoring-and-metrics.md) |
| Roll out keys across a dev org | [operations/key-management.md](operations/key-management.md) |
| Ship to CI across a fleet | [operations/ci-cookbook.md](operations/ci-cookbook.md) |
| Read the logs | [operations/logging.md](operations/logging.md) |
| Troubleshoot common issues | [operations/troubleshooting.md](operations/troubleshooting.md) |

### Reason about trust

| Question | Read |
|---|---|
| What attacks does ProvekIt actually catch? | [security/threat-model.md](security/threat-model.md) |
| What does `binaryCid` catch? | [security/what-binaryCid-catches.md](security/what-binaryCid-catches.md) |
| What does `binaryCid` **not** catch? | [security/what-binaryCid-does-not-catch.md](security/what-binaryCid-does-not-catch.md) |
| Solver as TCB (Z3 trust) | [security/solver-trust.md](security/solver-trust.md) |
| Lift adapter trust | [security/adapter-trust.md](security/adapter-trust.md) |
| Signature non-repudiation | [security/signature-and-non-repudiation.md](security/signature-and-non-repudiation.md) |
| Reporting a vulnerability | [security/reporting-vulnerabilities.md](security/reporting-vulnerabilities.md) |

### Learn end to end from a worked example

| Example | Why |
|---|---|
| [examples/parseInt-cross-domain.md](examples/parseInt-cross-domain.md) | the canonical bridge demo: JS `parseInt` and Rust `parse` both bridging to `ref-parseInt-v1` |
| [examples/supply-chain-attack-demo.md](examples/supply-chain-attack-demo.md) | demonstrates what `binaryCid` catches at build time |
| [examples/polyglot-microservices.md](examples/polyglot-microservices.md) | TS frontend + Python ML + Rust backend + Go gateway, all bridging to shared reference contracts |
| [examples/rust-crate-with-proptest.md](examples/rust-crate-with-proptest.md) | a real Rust crate, lifted, verified, published |
| [examples/npm-package-with-zod.md](examples/npm-package-with-zod.md) | a real npm package, lifted, verified, published |
| [examples/python-with-pydantic.md](examples/python-with-pydantic.md) | a real Python package, lifted, verified, published |
| [examples/java-with-bean-validation.md](examples/java-with-bean-validation.md) | a real JVM module, lifted, verified, published |

## Reference contracts

The bridge mechanism in the protocol depends on shared reference contracts: canonical anchors that multiple host-language implementations bridge to. The curated set lives in [reference-contracts/](reference-contracts/). Examples:

- `ref-parseInt-v1` (ECMA-262 `parseInt` reference)
- `ref-parseFloat-v1` (ECMA-262 `parseFloat` reference)
- `ref-malloc-v1` (POSIX `malloc` reference)
- `ref-ieee754-arithmetic-v1` (IEEE-754 arithmetic reference)

See [reference-contracts/README.md](reference-contracts/README.md) for what reference contracts are, why they matter, and how to propose a new one.

## Governance and protocol versions

This documentation describes protocol catalog v1.4.1, CID `blake3-512:dc2f42ff...` (full CID in [`reference/cids.md`](reference/cids.md)). v1.4.1 is a patch over v1.4.0 (re-syncs + clarifications, no protocol-level breaking changes); v1.4.0 mementos and `.proof` bundles remain valid forever against the bytes they were minted for. Verify your local install conforms via `provekit verify-protocol`. For protocol governance, version transitions, and conformance claims, see [governance/](governance/).

## Internal / project-meta

Project history, postmortems, retrospectives, and the historical `neurallog` spec live in [internal/](internal/). These are kept for context and are not part of the user-facing protocol.

## Status of this index

Many of the files this index points at are stubs or do not yet exist. The IA is intentionally complete so contributors can see where new content belongs. If a link 404s, the file is on the writing queue. See [contributing/overview.md](contributing/overview.md) for what's open.
