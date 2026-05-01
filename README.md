# ProvekIt

**Verify a petabyte of behavior by comparing 64 bytes.**

ProvekIt is a content-addressed verification protocol. Every contract, implication, and proof is a signed memento; every memento has a self-identifying CID; every verification is a hash comparison. The protocol version is itself a CID. There is no central registry, no trusted authority, no service to call. There is only math.

## The 30-second pitch

Modern dependency stacks are deep. A Rust project resolves to thousands of crates; an npm tree, tens of thousands. Verifying behavioral correctness across that stack with a tool that walks the AST or invokes a solver per call site is hopeless. ProvekIt collapses the problem: a library publishes signed contract mementos along with its bytes, a consumer's verifier loads them, and the handshake at every call site reduces to `memcmp(local, expected, 64) == 0`. Above the hash is math. Below the hash is physics. The hash itself is one CPU instruction.

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

## How verification scales

The handshake is the cost model. Three tiers, in order:

1. **Hash equality.** Publisher's post-hash equals consumer's pre-hash after canonicalization. `memcmp` returns zero. The call site is discharged for free.
2. **Cached implication.** A signed implication memento exists asserting `post → pre`. The verifier checks the Ed25519 signature once per `(post, pre)` pair and discharges every call site sharing that pair.
3. **Solver fallback.** Z3 is invoked once per genuinely-novel `(post, pre)` pair. On `unsat`, the verifier mints a fresh implication memento and either keeps it locally, ships it in the project's `.proof`, or pushes to a public implication server. The next verifier in the ecosystem hits Tier 2.

The lattice of cached implications grows monotonically. Cache invalidation is structurally absent: when bytes change, hashes change, and old mementos remain valid for old bytes (Corollary 3 of the lattice tractability theorem at CID `blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07`).

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
cargo build --release --manifest-path implementations/rust/Cargo.toml
cargo test  --release --manifest-path implementations/rust/Cargo.toml

# Go (three modules under implementations/go/)
cd implementations/go/provekit-ir-symbolic && go test ./...

# C++ peer (clang++ + vendored BLAKE3, openssl@3, nlohmann-json)
tools/build-cpp-self-contracts.sh --build-only

# TypeScript (pnpm workspace at the repo root)
pnpm install --frozen-lockfile
pnpm test

# C# peer (.NET 10)
dotnet test implementations/csharp/Provekit.sln

# Python lift adapter test suite
cd implementations/python/provekit-lift-py-tests && pip install -e . && pytest
```

### Known broken: TS launcher binaries

The shell launchers `bin/provekit.cjs` / `bin/provekit-lift.cjs` fail on Node 25 because of an `@ipld/dag-cbor` ESM-only + `tsx` CJS-bridge interaction. The vitest invocation `pnpm vitest run implementations/typescript/src/bin/mint-ts-self-contracts.test.ts` is the working invocation for the TS peer (Vitest's Vite ESM loader handles `dag-cbor` cleanly), and is the path CI uses. The launcher fix is tracked separately and is out of scope for the conformance gate.

## Documentation

- [PITCH.md](PITCH.md): the launch post.
- [PRODUCT.md](PRODUCT.md): what ProvekIt is, who it's for, what it complements.
- [ARCHITECTURE.md](ARCHITECTURE.md): the four-layer model, the handshake, the lattice.
- [THESIS.md](THESIS.md): the deeper architectural claim.
- [docs/getting-started.md](docs/getting-started.md): five-minute walkthrough.
- [docs/lift-adoption-paths.md](docs/lift-adoption-paths.md): per-source-library adoption guide.
- [docs/per-language-status.md](docs/per-language-status.md): kit, libs, and adapter matrix.
- [protocol/specs/](protocol/specs/): the canonical specs, addressed by CID.

## Lineage

Bitcoin proved you can mint trust without a mint. Git proved a content-addressed graph holds a software project's full history. BitTorrent proved a swarm can distribute petabytes without a server. IPFS proved that "the address is the content" generalizes. ProvekIt is one more application of the same primitive, applied to behavioral verification: contracts are mementos, mementos have CIDs, CIDs are addresses, addresses are eternal, and the protocol asks no one's permission to publish.
