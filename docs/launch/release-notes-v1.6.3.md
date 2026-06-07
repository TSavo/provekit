# Sugar v1.6.3: Protocol Evolution

**Catalog CID:** `blake3-512:dd0cc79889ee67d2594f5cfa20a191bafed15196fb2c5036f85deced7cd976055ae93825edebc10812b6fcf3c6ccf274fbc1137f32705aa0dc5938dc5825e31d`

**Version label:** `v1.6.3-2026-05-09`

**PEP migration witness:** v1.6.2 to v1.6.3 minted via Protocol Evolution Protocol. Witness CID and `ProtocolEvolutionBodyClaim` resolvable from the catalog graph.

## Verify this release

```sh
cargo run --release \
  --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify
```

Exit 0 iff every spec hashes to the value the catalog declares. The protocol verifies its own authority. Trust nothing else.

## What's new since v1.6.2

### Protocol surface

- **Protocol Evolution Protocol (PEP).** New extension protocol formalizes how protocol versions evolve. Each version is a signed content-addressed root; each migration is a witnessed edge with `ProtocolEvolutionBodyClaim` + `TruthDischargeWitness`. The `provekit protocol evolve` and `provekit protocol check-evolution` CLI verbs make migration first-class.
- **Content-Addressed CI Protocol.** Every CI gate becomes a typed claim root with explicit closure. Branch protection can require typed evidence, not just "checks passed."
- **PEP dogfood evolution checker.** The protocol uses its own evolution protocol to evolve. The v1.6.2 to v1.6.3 bump itself is a witnessed PEP migration; the dogfood is testable.
- **Conformance catalog-pinned across kits.** Per-kit conformance gates now resolve through the protocol catalog rather than per-language hand-pinning. One source of truth.

### Lifter family expansion

C lifter family extended (sparse contract lifter, assertions contract lifter, libclang AST backend, kernel compile context resolver). Java JUnit value-scope lifter shipped. Rust + C LSP forward-propagation floor in production. Polyglot bug-zoo verified across Java, TypeScript, C#, Go, and Rust.

### Menagerie

Six runnable destinations (was three at v1.6.2 cut):

| Destination | Status | Paper |
|---|---|---|
| bug-zoo | runnable | 07, 09 |
| supply-chain-rails | runnable | 06 |
| bridgeworks | runnable | 04 |
| protocol-switchyard | runnable (new at v1.6.3) | 10 |
| hashbound-mainline | planned | n/a |
| change-station | planned | 11 |

`protocol-switchyard` ships with a numbered walkthrough + four break scripts that prove which rail fires when each artifact is perturbed.

### Documentation

- **11-paper After-X arc** complete (Whitepaper, Bluepaper, Substrate-not-Blockchain, Vertical Stack, Witness Pluralism, After Reputation, After Verification, After Types, Lossy Boundary Compression, After Protocol Specs, After Commits).
- Papers index now cross-links each paper to its runnable menagerie counterpart.
- Menagerie toolchain prerequisites surfaced in `menagerie/README.md` + `menagerie/scripts/check-prereqs.sh` for cold-start visitors.

### Architectural clarifications

- Paper 03 §4: Turing-completeness gives sufficiency, not uniqueness. The substrate's universality at this layer is empirical, not a theorem about the design space.
- Paper 11 §8: CI bootstrap inherits the lifter's correctness. Refusal receipts catch known unknowns; lifter defects are unknown unknowns. The Bug Zoo loop is the falsifiability gate.

## Verify chain (historical catalogs)

```
v1.4.0  blake3-512:b0f2030d56c2fddf...
v1.4.1  blake3-512:dc2f42ff8a4a6628... (bluepaper freeze, May 3)
v1.5.0  blake3-512:540e8c1f5f7fea88...
v1.6.0  blake3-512:ce04a4053498...     (FloatSort + RegionSort)
v1.6.1  blake3-512:fa1fbf90b7f0...
v1.6.2  blake3-512:52bdb2be4b38...
v1.6.3  blake3-512:dd0cc79889ee...     <- this release
```

Each step is a witnessed PEP migration edge. The chain itself is the authority.

## First principle

*Supra omnia, rectum.* (T)

---

**Tagging command (when ready to ship):**

```sh
git tag -s v1.6.3 -m "Sugar v1.6.3: protocol catalog dd0cc79889ee..."
git push origin v1.6.3
gh release create v1.6.3 --notes-file .staged/release-notes-v1.6.3.md
```
