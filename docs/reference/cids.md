# Spec CIDs at HEAD

Every spec in ProvekIt is content-addressed by BLAKE3-512. This page lists the canonical CIDs at HEAD (protocol v1.4.1). Verify the local install conforms via `provekit verify-protocol`.

## Two hashing rules

Spec files use **raw-bytes** hashing: `BLAKE3-512(file_bytes)`.

The catalog itself uses **JCS-canonical** hashing: `BLAKE3-512(JCS(catalog-json))`. This is because the catalog is JSON; canonicalizing first means the CID survives whitespace / key-order edits.

The `tools/recompute-spec-cids/` crate implements both rules. The single command that verifies everything:

```sh
cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify
```

`--verify` reads every spec in raw bytes, hashes each, then reads the catalog, JCS-canonicalizes it, hashes that, and compares all values. Exit 0 iff every value matches.

## Pinned CIDs (v1.4.1)

| Document | CID |
|---|---|
| Protocol catalog (v1.4.1) | `blake3-512:9cb8600c84f682502f3b7e9a9f23b8138988bd1ffbcdcf3cd4e4b1d9ab386d3bb5076cd4dda8330edc24b43e62041a2da5153801001ed700d0be727073ceda67` |
| Canonicalization grammar | `blake3-512:4d8c2940c53a59c678c8fb65e33dc2cb0ae8ae8a283b97b9c69fd678565653d15e6ee9dc3ffc6a32dc1ff035821b0c1a006f0455498d2ea91faef845d7b39830` |
| Handshake algorithm | `blake3-512:acbf67dda9373c648e591d8ad74b8f8d56f4c92ba9c82bdc6690dc521e6f17012dd195e98a96b099090eeeb5a424312d90ff441c882d0e317a190561aa1a6925` |
| IR formal grammar | `blake3-512:6c0127e0d24946d7be75861db20507ccdcfdf968d3333f8aa34083e849d8238d73b3acfaa31880648995a024112182ed6b6002cd489548b4b18f5d4c3768dd96` |
| Lattice tractability theorem | `blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07` |
| Memento envelope grammar | `blake3-512:58bba3e1a9f6439eac5cb0c681faf65d38de9e6b8ad539854acda451ca67562a9d238eb95a5d7df2c0776657015fa026c51059dff61e1ba9aa2438b57425d6a5` |
| Proof file format | `blake3-512:7bb4589af25c6c3992520494869bbbe4cfbcf7a77b91ebd61d6327e78699ef16cd5bc34afbe4cdf88a717c055c16536b5106bc4dca2d9d6b5cfcc1eede68e1b3` |
| Proof substrate | `blake3-512:ad53d6c59ee08270a48715376cc211f964ff44a55b3318d68a402e9c915ff593d5a5bbbd424f7777e2bcfe89d6c5bd2b49efcb5aae7de24752f3bcabb90484ae` |
| Self-contracts (stable; v1.1.0+) | `blake3-512:a0f58941758d709739759cf166bf9cb73794958144e213eccfb28fbf5791ca824ce53da0c6ba801cca2b53400324a094f510d4bbc41bc6b73b17e486ad3838ab` |
| Signatures and non-repudiation | `blake3-512:8b71229fcb7413f18a93a9b260012298311c1ce754850ee717780c181f1fda39a6600b2e5069e775cd7dd15e8c81e40b47bf7585aa0b23ab76c112c85116365c` |

## v1.4.0 additions

The v1.4.0 bump is additive over v1.3.1. New specs published with v1.4.0:

- `substrate-layers-envelope-header-body` (`2026-05-03-substrate-layers-envelope-header-body.md`)
- `contract-cid-vs-attestation-cid` (`2026-05-03-contract-cid-vs-attestation-cid.md`)
- `contract-set-extension` (`2026-05-03-contract-set-extension.md`)
- `version-chains-pinning` (`2026-05-03-version-chains-pinning.md`)
- `bridge-target-dimensionality` (`2026-05-03-bridge-target-dimensionality.md`)
- `bridge-linkage-protocol` (`2026-05-03-bridge-linkage-protocol.md`)
- `binary-attestation-protocol` (`2026-05-02-binary-attestation-protocol.md`)
- `bundle-attestation-protocol` (`2026-05-02-bundle-attestation-protocol.md`)
- `opacity-manifest-grammar` (`2026-05-02-opacity-manifest-grammar.md`)

The full list of v1.4.1 spec CIDs is in `protocol/specs/2026-04-30-protocol-catalog.json`. Recompute locally to verify.

## Per-kit self-contracts CIDs (v1.4.1)

Each conformant peer ships hand-written contracts about its own public surface, mints them as signed mementos under the foundation key, and bundles into a `.proof` whose filename IS its catalog CID.

| Kit | Self-contracts CID | Mint command |
|---|---|---|
| Rust | `blake3-512:3c905e3b27d279fb5d11e49af10d8f1d8c83aec207d0bb695d08cacba5c3192e56457d4683d93e71ffd18bd0acb65b72a2b49404490bce809e8dc1df7fd0bac8` | `cargo build --release --manifest-path implementations/rust/Cargo.toml --bin mint-self-contracts && implementations/rust/target/release/mint-self-contracts` |
| Go | `blake3-512:906fa4f3ca32d97710e327c9e6e914e5c476a3cfdc326459b31dade24d9625c96f7f0595e3d91f316f73e2709a7f05ac79dd0ca768b6ff23cc2b384923487ac3` | `cd implementations/go/provekit-self-contracts && go run ./cmd/mint-go-self-contracts` |
| C++ | `blake3-512:9335e6376d776819cfd3b2458da29bc258e7c2ebaad542a8613dd84f50c51c31d6e1a4346cea3903b8ad12294d96aef445d0ed838aa630835b9be0bc17e62842` | `tools/build-cpp-self-contracts.sh /tmp/provekit-cpp-self-out` |
| TypeScript | `blake3-512:449339930add6457bf25542f2117a025daada4a4bd1de704737750ad6d1c1be814c284d31bb97159ca0b2d2c52f8c043a64533d3432195f5a0f338c5d4904d44` | `pnpm vitest run implementations/typescript/src/bin/mint-ts-self-contracts.test.ts --reporter=verbose` |
| C# | `blake3-512:45d7cdbd0d5bfba5a1ee9e8386eb4d7dc1eab0882105753504a1f5c06de6f9fc4bd7038f56c7fcea693b152e2ab83de40ca4964a920816142ea43d5b9076415c` | `dotnet run --project implementations/csharp/Provekit.SelfContracts -- /tmp/csharp-self-out` |

Two runs producing the same CID is the framework verifying its own canonicalization is deterministic. If a value above does not match what your local mint produces, your bytes are not the bytes this protocol version was published against.

## Bluepaper recursive-verification

The protocol catalog's CID is the protocol version. Verifying the catalog is the act of running the protocol; running the protocol verifies the catalog. There is no external authority. The bluepaper at [`../papers/02-bluepaper.md`](../papers/02-bluepaper.md) closes with this recursive verification recipe. Run `--verify`. If the computed catalog CID matches `9cb8600c...`, the bluepaper has just verified its own authority over the bytes you have.

## Read next

- [`../papers/02-bluepaper.md`](../papers/02-bluepaper.md) — full formal protocol specification with all spec CIDs.
- [`../explanation/architecture.md`](../explanation/architecture.md) — protocol mechanics.
- [`../contributing/build.md`](../contributing/build.md) — how to recompute via `make conformance`.
