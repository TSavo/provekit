# ProvekIt

**Verify a petabyte of behavior by comparing 64 bytes.**

ProvekIt is **not** a formal verification framework. It is **a protocol for content-addressing formal verifications**. The same primitive Bitcoin used for currency, Git uses for source history, BitTorrent uses for content distribution, and IPFS uses for the addressable web — applied to behavioral verification. Every contract, implication, and proof is a signed memento; every memento has a self-identifying CID; every verification is a hash comparison. The protocol version is itself a CID. There is no central registry, no trusted authority, no service to call. There is only math.

## I want to...

| Goal | Start here |
|---|---|
| Try it in **Rust** | [docs/tutorials/rust.md](docs/tutorials/rust.md) |
| Try it in **TypeScript** | [docs/tutorials/typescript.md](docs/tutorials/typescript.md) |
| Try it in **Python** | [docs/tutorials/python.md](docs/tutorials/python.md) |
| Try it in **Java / JVM** | [docs/tutorials/java.md](docs/tutorials/java.md) |
| Try it in **C#** | [docs/tutorials/csharp.md](docs/tutorials/csharp.md) |
| Try it in **Ruby** | [docs/tutorials/ruby.md](docs/tutorials/ruby.md) |
| Try it in **Zig** | [docs/tutorials/zig.md](docs/tutorials/zig.md) |
| Try it in **Go / C++ / Swift / C** | [docs/tutorials/](docs/tutorials/) (kits ship; lift adapters land in v1.2) |
| See the polyglot demo | [docs/tutorials/polyglot-stack.md](docs/tutorials/polyglot-stack.md) |
| Get red squigglies in my IDE | [docs/how-to/ide-integration/](docs/how-to/ide-integration/) |
| Ship a `.proof` alongside my package | [docs/how-to/publishing-a-proof.md](docs/how-to/publishing-a-proof.md) |
| Verify a dependency's `.proof` | [docs/how-to/consuming-a-proof.md](docs/how-to/consuming-a-proof.md) |
| Add my language | [docs/contributing/porting-to-a-new-language.md](docs/contributing/porting-to-a-new-language.md) |
| Write a lift adapter | [docs/contributing/writing-a-lift-adapter/](docs/contributing/writing-a-lift-adapter/) |
| Understand the thesis | [docs/explanation/thesis.md](docs/explanation/thesis.md) |
| Read the whitepaper (executive summary) | [docs/papers/01-whitepaper.md](docs/papers/01-whitepaper.md) |
| Read the bluepaper (formal protocol spec) | [docs/papers/02-bluepaper.md](docs/papers/02-bluepaper.md) |
| Read the manifesto (substrate, not blockchain) | [docs/papers/03-substrate-not-blockchain.md](docs/papers/03-substrate-not-blockchain.md) |
| Read the vertical-stack + standardization paper | [docs/papers/04-vertical-stack-and-standardization.md](docs/papers/04-vertical-stack-and-standardization.md) |
| Compare to SLSA / Sigstore / SBOM | [docs/explanation/compared-to/](docs/explanation/compared-to/) |
| Reason about trust | [docs/security/threat-model.md](docs/security/threat-model.md) |
| Look up a CLI flag | [docs/reference/cli/](docs/reference/cli/) |
| Look up an IR node | [docs/reference/ir/](docs/reference/ir/) |
| Look up spec CIDs | [docs/reference/cids.md](docs/reference/cids.md) |
| Read everything | [docs/index.md](docs/index.md) |

## What is it?

Modern dependency stacks are deep. A Rust project resolves to thousands of crates; an npm tree, tens of thousands. Verifying behavioral correctness across that stack with a tool that walks the AST or invokes a solver per call site is hopeless. ProvekIt collapses the problem: a library publishes a signed `.proof` catalog alongside its bytes, a consumer's verifier loads it, and the handshake at every call site reduces to `memcmp(local, expected, 64) == 0`. Above the hash is math. Below the hash is physics. The hash itself is one CPU instruction.

The `.proof` file IS the package. It contains contracts, bridges, verification evidence, and optionally a content-addressed reference to the compiled binary. The filename IS the trust root: `<cid>.proof`, where `cid` is the BLAKE3-512 of the file's bytes. Change any bit, the CID changes, the old proof is still valid, the new one must be re-verified.

ProvekIt does not compete with `proptest`, `contracts`, `kani`, `prusti`, `hypothesis`, `pydantic`, `zod`, `class-validator`, `bean-validation`, JML, DataAnnotations, or `active_model`. It sits beneath them. The thing that produces a verification — Z3, Coq, Lean, F\*, CBMC, Kani, Prusti, hand-written annotation — is the formal verification framework. ProvekIt is the substrate over which those frameworks publish their findings: portable, signed, content-addressed, federated. Whatever annotation library a codebase already uses, the lift adapter promotes those annotations to canonical IR, hashes the IR, signs the memento. Authoring stays where the developer already is. Publication, distribution, and verification move underneath.

For the deeper claim, read [docs/explanation/thesis.md](docs/explanation/thesis.md). For end-to-end mechanics, [docs/explanation/architecture.md](docs/explanation/architecture.md). For what it replaces and complements, [docs/explanation/product.md](docs/explanation/product.md).

## Status

- **Protocol catalog**: v1.4.1 (patch over v1.4.0; v1.4.0 mementos and `.proof` bundles remain valid)
- **Catalog CID**: `blake3-512:dc2f42ff8a4a66289cc19bfbd628898b8bd8e61d2148ecf609324cc2421c5c440a6c0e70e20ffbecabeb78e0253101d72823b7e3ab120a4d56cb67c8e31dc641`
- **What's new in v1.4.1 (patch)**: three property re-bakes resolving v1.4.0 spec ambiguities. (a) `proof-file-format` §6.1 added: pins the v1.4 substrate-layers compatibility cut for catalog mementos and the catalog `header.cid` recipe as `BLAKE3-512(JCS(sorted_member_cids))`. (b) `memento-envelope-grammar` gets a supersession note pointing at `substrate-layers-envelope-header-body` for v1.4-and-later mementos. (c) `ir-formal-grammar` re-baked after the Locus type addition. Plus one non-cataloged clarification: `bridge-linkage-protocol` §1 DerivedBridge `schemaVersion` corrected from `"2"` to `"1"` consistent with substrate-layers and bridge-target-dimensionality. v1.4.0 catalog CID `b0f2030d...` stays attestable for anyone pinned.
- **What v1.4.0 introduced (still applies)**: substrate layering (envelope/header/body cut), contract-CID vs. attestation-CID separation, contract-set extension (verifiable semver-minor), version-chain pinning (package-manager replacement), bridge target dimensionality (tagged-union targets, no placeholder strings), three-axis pinning at the consumer surface (`contractCid`, `witnessCid`, `binaryCid`). See [docs/papers/03-substrate-not-blockchain.md](docs/papers/03-substrate-not-blockchain.md) §11–§12 for the multi-dimensional address-space framing this operationalizes.
- **Canonical implementation**: Rust (`cargo install provekit`)
- **Conforming implementations** today: Rust, TypeScript, Python, Java, C#, Ruby, Zig, Go, C++, Swift, C. Coverage varies; see [docs/reference/per-language-status.md](docs/reference/per-language-status.md) and [docs/reference/per-adapter-coverage.md](docs/reference/per-adapter-coverage.md).
- **Conformance gate**: every kit's mint must match a pinned content-addressed CID before `make ci` is green.

The protocol is content-addressed end to end. v1.1.0's canonical name is its own catalog hash. Anyone with the spec bytes can verify that label locally. No central party decides what v1.1.0 means; the bytes do.

## Quick install (Rust, canonical)

```bash
cargo install provekit
provekit verify-protocol
cd path/to/your-rust-crate
cargo provekit-lift
provekit prove
```

`provekit verify-protocol` confirms the local install conforms to the expected protocol catalog CID. `cargo provekit-lift` walks the workspace, runs every registered lift adapter, and emits a `.proof` catalog of signed contract mementos. `provekit prove` runs the three-tier handshake and reports the discharge breakdown. Any of these can fail closed; none requires the network.

For other host languages, see the tutorials above. The Rust CLI is the canonical implementation; non-Rust kits use it for verification today.

## Building from source

If you are working on ProvekIt itself (kit, lift adapter, prover backend, spec change), see [docs/contributing/build.md](docs/contributing/build.md) for the polyglot Make targets, system dependencies, and per-implementation build commands. The conformance gate (`make ci`) enforces byte-determinism across every implementation.

## License

See [LICENSE](LICENSE).
