# ProvekIt

**Verify a petabyte of behavior by comparing 64 bytes.**

ProvekIt is a content-addressed verification protocol. Every contract, implication, and proof is a signed memento; every memento has a self-identifying CID; every verification is a hash comparison. The protocol version is itself a CID. There is no central registry, no trusted authority, no service to call. There is only math.

## The 30-second pitch

Modern dependency stacks are deep. A Rust project resolves to thousands of crates; an npm tree, tens of thousands. Verifying behavioral correctness across that stack with a tool that walks the AST or invokes a solver per call site is hopeless. ProvekIt collapses the problem: a library publishes a signed `.proof` catalog alongside its bytes, a consumer's verifier loads it, and the handshake at every call site reduces to `memcmp(local, expected, 64) == 0`. Above the hash is math. Below the hash is physics. The hash itself is one CPU instruction.

The `.proof` file IS the package. It contains contracts, bridges, verification evidence, and optionally a content-addressed reference to the compiled binary. The filename IS the trust root: `<cid>.proof`, where `cid` is the BLAKE3-512 of the file's bytes. Change any bit, the CID changes, the old proof is still valid, the new one must be re-verified.

ProvekIt does not compete with `proptest`, `contracts`, `kani`, `prusti`, `hypothesis`, `pydantic`, `zod`, `class-validator`, or `bean-validation`. It sits beneath them. Whatever annotation library a codebase already uses, the lift adapter promotes those annotations to content-addressed signed contracts, with no rewrites and no parallel spec to maintain. Authoring stays where the developer already is. Verification moves underneath.

The protocol is content-addressed end to end. v1.1.0's canonical name is its own catalog hash: `blake3-512:9d57c5e47083b92e8cc5dab365a718fc0afee6556d34ffe40b303dd7ad4d9caa88dbbc6248e318cc76e57b30a0b2ad49f6f9dbf1916ac164a89df44324d6c106`. Anyone with the spec bytes can verify that label locally. No central party decides what v1.1.0 means; the bytes do.

## The 5-minute install

```bash
cargo install provekit
provekit verify-protocol
cd path/to/your-rust-crate
cargo provekit-lift
provekit prove
```

`provekit verify-protocol` confirms the local install conforms to the expected protocol catalog CID. `cargo provekit-lift` walks the workspace, runs every registered lift adapter (today: `proptest`, `contracts`), and emits a `.proof` catalog of signed contract mementos. `provekit prove` walks the catalog, runs the three-tier handshake, and reports the discharge breakdown. Any of these can fail closed; none requires the network.

For a fuller walkthrough, see [docs/getting-started.md](docs/getting-started.md).

## What you get

- **A protocol, not a tool.** The CLI is one implementation. The protocol is the spec catalog at the CID above. Conforming implementations exist or are planned for Rust, TypeScript, Go, and C++.
- **Lift, don't author.** Existing annotation libraries become contract sources via per-library lift adapters. Today: `proptest` and `contracts` for Rust. Planned for v1.2: `kani`, `prusti` (Rust); `zod`, `class-validator`, `fast-check` (TypeScript); `deal`, `hypothesis`, `pydantic` (Python); `bean-validation`, JML, Cofoja (Java); `go-playground/validator` (Go).
- **A petabyte-to-64-bytes ratio.** Verification of an arbitrarily-deep dependency stack reduces to a 64-byte hash comparison per call site. Tier 1 of the handshake is one CPU instruction. Tier 2 is one signature verification on a cached implication memento. Tier 3 invokes Z3 once per novel `(post, pre)` pair, mints the result, and every future verifier hits Tier 2.
- **No database.** The "registry" is the BLAKE3-512 hashspace plus whatever bytes you and your peers have published. Same lineage as Bitcoin, Git, IPFS, BitTorrent: addresses are mathematical, populated points are sparse, no central party holds a master copy.
- **Compile-time checks where the host language allows it.** The `provekit-build` integration (in flight, planned for v1.2) lifts contract violations into compile-time errors in Rust. The proof gate becomes a smarter type system extension, not a runtime probe.
- **Hash-bounded cross-domain verification.** The `#[provekit::implement(target = "...")]` macro lets a developer explicitly bind a function to a contract by CID. The DAG of bridges provides transitive verification across platforms: EVM bytecode, Solana BPF, WASM, native binaries — all proven against the same reference contract.

## How verification scales

The handshake is the cost model. Three tiers, in order:

1. **Hash equality.** Publisher's post-hash equals consumer's pre-hash after canonicalization. `memcmp` returns zero. The call site is discharged for free.
2. **Cached implication.** A signed implication memento exists asserting `post → pre`. The verifier checks the Ed25519 signature once per `(post, pre)` pair and discharges every call site sharing that pair.
3. **Solver fallback.** Z3 is invoked once per genuinely-novel `(post, pre)` pair. On `unsat`, the verifier mints a fresh implication memento and either keeps it locally, ships it in the project's `.proof`, or pushes to a public implication server. The next verifier hits Tier 2.

The lattice of cached implications grows monotonically. Cache invalidation is structurally absent: when bytes change, hashes change, and old mementos remain valid for old bytes (Corollary 3 of the lattice tractability theorem at CID `blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07`).

## The `.proof` file: the package IS the proof

A `.proof` file is a CBOR-encoded catalog of mementos, addressed by its own CID. It ships alongside (or replaces) traditional package manifests:

```
my-package/
├── my-package.proof      ← THIS IS THE PACKAGE
│   ├── contracts: [...]
│   ├── bridges: [...]
│   ├── binaryCid: "bafy..."     ← optional: pins compiled artifact
│   └── metadata: {...}          ← decorative, signed, non-normative
├── src/
└── ...
```

The `binaryCid` field is the supply chain anchor: when present, the framework checks that the running binary's hash matches before trusting any claims. A compiler backdoor, runtime patch, or dependency injection changes the binary hash, the proof fails, and the build breaks.

Bridges inside the `.proof` bundle carry `sourceContractCid` and `targetProofCid`, making cross-bundle lookup explicit:

```json
{
  "kind": "bridge",
  "sourceContractCid": "bafy...myParseInt-v1",
  "targetContractCid": "bafy...ref-parseInt-v1",
  "targetProofCid": "bafy...ecma262-v14-proof"
}
```

The framework fetches the target `.proof` by CID, finds the contract inside it, and verifies the implication. Every hop is content-addressed. Every hop is a hash boundary.

## Building

ProvekIt is a five-language polyglot: Rust, Go, C++, TypeScript, C#. The cross-language conformance gate runs the same way locally and in CI: every peer mints its own self-contracts under the foundation key, and every minted catalog must match the pinned content-addressed CID before the build is green.

The contract is the top-level [`Makefile`](Makefile). If `make ci` is green, the protocol is byte-deterministic across all five peers on the host and every native test suite passes. The CI workflow at [`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs the same `make ci` on `ubuntu-latest`.

```sh
make help          # available targets
make ci            # full gate (conformance + every language's tests)
make conformance   # catalog + protocol + 5 mints match pinned CIDs
make all-mint      # run all 5 mint commands; print CIDs
make test-all      # run all language-native test suites
make clean         # remove all build artifacts
```

### System dependencies

| Package          | macOS (Homebrew)                    | Ubuntu / Debian                                |
|------------------|-------------------------------------|------------------------------------------------|
| Rust stable      | `rustup install stable`             | `rustup install stable`                        |
| Go 1.22+         | `brew install go`                   | `sudo apt install golang-1.22`                 |
| .NET 10 SDK      | `brew install --cask dotnet-sdk`    | Microsoft `packages-microsoft-prod` apt repo   |
| Node 22 + pnpm   | `brew install node@22 pnpm`         | `nodesource` apt repo + `npm i -g pnpm`        |
| Python 3.12      | `brew install python@3.12`          | `sudo apt install python3.12 python3-pip`      |
| OpenSSL 3        | `brew install openssl@3`            | `sudo apt install libssl-dev`                  |
| nlohmann-json    | `brew install nlohmann-json`        | `sudo apt install nlohmann-json3-dev`          |
| BLAKE3           | vendored at `tools/blake3-vendored` | vendored at `tools/blake3-vendored`            |

BLAKE3 is vendored as portable C source under `tools/blake3-vendored/` (BLAKE3 1.8.5, Apache-2.0). The C++ build script compiles it with all SIMD paths disabled, so no system BLAKE3 install is required and the build is hermetic on any host with `clang`.

### Per-language quickstart

```sh
# Rust workspace + Rust tools
cargo install --path implementations/rust/provekit-cli

# TypeScript packages
cd implementations/typescript && pnpm install && pnpm build

# Go tools
cd implementations/go && go build ./...

# C++ tools (requires clang, OpenSSL, nlohmann-json)
cd implementations/cpp && make

# C# tools (requires .NET 10 SDK)
cd implementations/csharp && dotnet build
```

## Specs and CIDs

| Document | CID |
|---|---|
| Protocol catalog (v1.1.0) | `blake3-512:9d57c5e47083b92e8cc5dab365a718fc0afee6556d34ffe40b303dd7ad4d9caa88dbbc6248e318cc76e57b30a0b2ad49f6f9dbf1916ac164a89df44324d6c106` |
| IR formal grammar | `blake3-512:6c0127e0d24946d7be75861db20507ccdcfdf968d3333f8aa34083e849d8238d73b3acfaa31880648995a024112182ed6b6002cd489548b4b18f5d4c3768dd96` |
| Proof file format | `blake3-512:7bb4589af25c6c3992520494869bbbe4cfbcf7a77b91ebd61d6327e78699ef16cd5bc34afbe4cdf88a717c055c16536b5106bc4dca2d9d6b5cfcc1eede68e1b3` |
| Handshake algorithm | `blake3-512:acbf67dda9373c648e591d8ad74b8f8d56f4c92ba9c82bdc6690dc521e6f17012dd195e98a96b099090eeeb5a424312d90ff441c882d0e317a190561aa1a6925` |
| Lattice tractability theorem | `blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07` |
| Signatures and non-repudiation | `blake3-512:8b71229fcb7413f18a93a9b260012298311c1ce754850ee717780c181f1fda39a6600b2e5069e775cd7dd15e8c81e40b47bf7585aa0b23ab76c112c85116365c` |

Every spec is in `protocol/specs/`. Every spec has a CID. The `tools/recompute-spec-cids/` crate re-derives every CID and fails on any drift. Run `cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify` to check conformance locally.

## Read further

- [ARCHITECTURE.md](ARCHITECTURE.md) for the protocol mechanics.
- [THESIS.md](THESIS.md) for the deeper architectural claim.
- [PRODUCT.md](PRODUCT.md) for what ProvekIt replaces and complements.
- [docs/getting-started.md](docs/getting-started.md) for the full install path.
- [docs/per-language-status.md](docs/per-language-status.md) for kit and adapter coverage.
- [protocol/specs/](protocol/specs/) for the canonical spec set, addressed by CID.
