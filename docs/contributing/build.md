# Building ProvekIt from source

ProvekIt is a multi-language polyglot. The main implementations today: Rust, Go, C++, TypeScript, C#, Python, Java, Ruby, Zig, Swift, C, PHP. The cross-language conformance gate runs the same way locally and in CI: every peer mints its own self-contracts under the foundation key, and every minted catalog must match the pinned content-addressed CID before the build is green.

The contract is the top-level [`Makefile`](../../Makefile). If `make ci` is green, the protocol is byte-deterministic across every peer on the host and every native test suite passes. The CI workflow at [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) runs the same `make ci` on `ubuntu-latest`.

## Make targets

```sh
make help          # available targets
make ci            # full gate (conformance + every language's tests)
make conformance   # catalog + protocol + N mints match pinned CIDs
make all-mint      # run all mint commands; print CIDs
make test-all      # run all language-native test suites
provekit ci ...    # CICP supply-chain admission and result witnesses
provekit zoo ...   # Bug Zoo specimen/exposure/dropper checks
make clean         # remove all build artifacts
```

## System dependencies

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

## Per-implementation build

Each implementation can be built independently. This is for contributors working on a specific kit; end users should follow the per-language tutorial in [docs/tutorials/](../tutorials/) instead.

```sh
# Rust workspace + Rust tools (canonical CLI)
cargo install --path implementations/rust/provekit-cli

# TypeScript packages
cd implementations/typescript && pnpm install && pnpm build

# Go tools
cd implementations/go && go build ./...

# C++ tools (requires clang, OpenSSL, nlohmann-json)
cd implementations/cpp && make

# C# tools (requires .NET 10 SDK)
cd implementations/csharp && dotnet build

# Python tools
cd implementations/python && pip install -e .

# Java tools (Maven multi-module)
cd implementations/java && mvn install

# Ruby tools (requires Ruby 3+)
cd implementations/ruby && bundle install

# Zig tools
cd implementations/zig && zig build

# Swift tools
cd implementations/swift && swift build
```

## Conformance gate

The conformance gate is the heart of the polyglot story. Every implementation mints a self-contracts package; every minted package must hash to a CID pinned in the protocol catalog. The gate runs in two layers:

1. **Catalog conformance.** `tools/recompute-spec-cids/` re-derives every spec's CID from the spec bytes and fails on any drift.
   ```sh
   cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify
   ```
2. **Cross-kit conformance.** Each kit mints its self-contracts under the foundation key. The minted CIDs are pinned per kit; CI fails if any kit drifts.
   ```sh
   make conformance
   ```

Additional protocol/tooling checks now run in CI:

- **Proof protocol conformance.** `.proof` fixtures under `protocol/conformance/proof-protocol/` are checked by `provekit proof`.
- **CICP vector conformance.** Every language library that emits CICP bodies must derive the same golden-vector CIDs in `protocol/conformance/cicp/`.
- **CICP supply-chain admission.** The GitHub workflow computes kit blast radii, tries reuse only against checked-in accepted witnesses, and uploads candidate result witnesses for review when reuse is refused.
- **Bug Zoo.** `provekit zoo` verifies exposed ProofIR equivalence and optional dropper closure receipts for checked-in specimens.

If you are adding a new implementation, see [porting-to-a-new-language.md](porting-to-a-new-language.md) for how the conformance harness picks up your kit.

## Where to go next

- [contributing/overview.md](overview.md) for the contributor on-ramp.
- [contributing/porting-to-a-new-language.md](porting-to-a-new-language.md) for adding a new host language.
- [contributing/writing-a-kit/](writing-a-kit/) for the six-step kit author guide.
- [contributing/writing-a-lift-adapter/](writing-a-lift-adapter/) for the five-step lift adapter author guide.
- [contributing/release-process.md](release-process.md) for cutting a release.
