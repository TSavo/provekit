# ProvekIt

**Cross-language compile-time correctness verification.**

Annotations in every language already express contracts: Rust's `assert!`, Go's `//provekit:contract` comments, Java's `@NotNull`, Python type hints. Those annotations stop being enforced the moment your code crosses a language boundary via cgo, JNI, ctypes, or WASM imports. ProvekIt lifts them into a common predicate substrate, derives bridges at every cross-language call site, and surfaces violations as red squiggles in the developer's IDE. The annotation was always there. ProvekIt makes it load-bearing across language boundaries.

## Pick your path

| I want red squiggles | I want to extend ProvekIt |
| --- | --- |
| You write rust/go/python/etc. code with the annotations your language already has. ProvekIt's editor LSP shows you cross-language contract violations as red squiggles in real time. No config beyond pointing your editor at the LSP binary. | You are building a new lifter, a new kit, a new spec, or contributing to the substrate. The architectural derivation, path-to-default strategy, and protocol catalog are your primary reading. |
| [docs/quickstart-end-user.md](docs/quickstart-end-user.md) | [docs/quickstart-extender.md](docs/quickstart-extender.md) |

## What the substrate does

Every ProvekIt kit (rust, go, python, cpp, c#, java, ruby, zig, swift, ts, c) lifts source annotations into ProvekIt IR: a content-addressed predicate language where the same logical predicate produces the same 64-byte BLAKE3-512 hash regardless of which language wrote it.

A lifter walks your source, extracts contracts (pre/post predicates) and call edges (which function calls which). The linker takes the union of all kits' output and derives a bridge for each cross-language call edge: does the caller's postcondition establish the callee's precondition? If not, the bridge derivation fails. That failure is a `linker-error` memento. The LSP server converts it to a diagnostic and the editor shows a red squiggle.

The `.proof` bundle that ships alongside your binary is the frozen IDE state at ship time: every squiggle was green, every contract verified, every cross-language call's bridge derivation succeeded. A consumer recomputes the `linkBundleCid` from their copy of the code and checks byte-equality. No re-running. The proof is the snapshot.

## Status

See [docs/per-language-status.md](docs/per-language-status.md) for the complete matrix. Summary:

| Kit | Self-contracts | Lift-plugin-protocol bridges | LSP plugin |
|---|---|---|---|
| Rust | full conformance | full (source of truth) | shipping |
| Go | full conformance | in progress | planned |
| C# | full conformance | not started | shipping |
| Ruby | not started | not started | shipping |
| Zig | not started | not started | shipping |
| Python | in progress | in progress | shipping |
| TypeScript | full conformance | in progress | planned |
| C++ | full conformance | not started | planned |
| Java | not started | not started | planned |
| Swift | not started | not started | planned |

## Install path

This project is **build-from-source only**. Crates.io publishing is on the roadmap; until then see [docs/quickstart-end-user.md](docs/quickstart-end-user.md) for build instructions.

The core binary is:

```sh
cargo install --path implementations/rust/provekit-cli
```

The LSP server is:

```sh
cargo install --path implementations/rust/provekit-lsp
```

The daemon is:

```sh
cargo install --path implementations/rust/provekit-linkerd
```

## Reading order if you want to understand the architecture

1. `docs/launch/substrate-not-blockchain.md` — manifesto §1-§12, the substrate posture
2. `docs/launch/the-pieces-on-the-table.md` — twelve-step architectural derivation
3. `docs/launch/path-to-default.md` — adoption strategy
4. `protocol/specs/` — normative specs, each addressed by content-hash CID

## Building

The top-level `Makefile` is the gate. If `make ci` is green, every peer's self-contracts round-trip to its pinned CID, the catalog hash matches, and every native test suite passes.

```sh
make help          # available targets
make ci            # full gate (conformance + every language's tests)
make conformance   # catalog + protocol + 5 mint CIDs + self-contract tests
make all-mint      # run all 5 mint commands; print CIDs
make test-all      # run all language-native test suites
make build-rust    # cargo build --release (workspace + tools)
make clean         # remove build artifacts
```

## System dependencies

| Package | macOS | Ubuntu / Debian |
|---|---|---|
| Rust stable | `rustup install stable` | `rustup install stable` |
| Go 1.22+ | `brew install go` | `sudo apt install golang-1.22` |
| .NET 10 SDK | `brew install --cask dotnet-sdk` | Microsoft apt repo |
| Node 22 + pnpm | `brew install node@22 pnpm` | nodesource + `npm i -g pnpm` |
| Python 3.12 | `brew install python@3.12` | `sudo apt install python3.12` |
| OpenSSL 3 | `brew install openssl@3` | `sudo apt install libssl-dev` |
| nlohmann-json | `brew install nlohmann-json` | `sudo apt install nlohmann-json3-dev` |
| BLAKE3 | vendored at `tools/blake3-vendored` | vendored at `tools/blake3-vendored` |

BLAKE3 is vendored as portable C source under `tools/blake3-vendored/` (BLAKE3 1.8.5, Apache-2.0). The C++ build compiles it with all SIMD paths disabled, so no system BLAKE3 install is required.

## Read further

- [docs/quickstart-end-user.md](docs/quickstart-end-user.md) — get a red squiggle in 10 minutes
- [docs/quickstart-extender.md](docs/quickstart-extender.md) — write a new kit lifter or protocol spec
- [docs/per-language-status.md](docs/per-language-status.md) — kit and adapter coverage by language
- [docs/lift-adoption-paths.md](docs/lift-adoption-paths.md) — per-source-library lift adapter guide
- [ARCHITECTURE.md](ARCHITECTURE.md) — four-layer model, handshake, lattice tractability theorem
- [THESIS.md](THESIS.md) — the deeper architectural claim
- [protocol/specs/](protocol/specs/) — canonical specs, each content-addressed by CID
