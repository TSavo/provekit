# ProvekIt v1.6.4: Pattern Predicates and Contract Composition

**Catalog CID:** `blake3-512:09ccf7b1464622eceb4ac0e9bae3b435ba92d87c19e89f93724e6be75f4afce9eb3dedb7b8ebe2536de054143efefcb3cb622e6e5b4140bb26e6156a9bc9adf3`

**Version label:** `v1.6.4-2026-05-09`

**PEP migration witness:** v1.6.3 to v1.6.4 minted via Protocol Evolution Protocol. Witness CID and `ProtocolEvolutionBodyClaim` resolvable from the catalog graph.

## Verify this release

```sh
cargo run --release \
  --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify
```

Exit 0 iff every spec hashes to the value the catalog declares. The protocol verifies its own authority. Trust nothing else.

## What's new since v1.6.3

### Protocol surface

- **Pattern Predicate Protocol (PPP).** New draft extension protocol. Names how an editorially-defined bug class compiles to a content-addressed substrate query whose result-set delta discharges an FRP receipt's policy. PPP closes the gap between pattern catalog entries and FRP closure: pattern, predicate, query application, closure witness, FRP receipt, proofchain head. Spec at `protocol/specs/2026-05-09-pattern-predicate-protocol.md`. Reference tooling at `menagerie/pattern-predicate-protocol/`.
- **Contract Composition Protocol (CCP).** New draft extension protocol. Names canonical contract composition over the existing handshake tier 2 cache: atomic contract mementos compose at function call sites into `ComposedFunctionContract` mementos whose CIDs the cache reuses for O(1) discharge of structurally-equivalent chains across any future program in any language. Names the canonical compose primitive in libprovekit and its FFI / CLI / direct-link binding modes. Spec at `protocol/specs/2026-05-09-contract-composition-protocol.md`.
- **PPP / CCP cross-link.** PPP additively extends its v1 substrate schema with `effects` and `composed_contracts` relations populated by CCP. The two protocols compose: PPP queries can range over CCP composition state.

### Implementation extensions

- `libprovekit::compose` and `libprovekit::ffi` — canonical composition entry points exposed via direct-link, FFI, and CLI binding modes. Effects tracking is the per-language prerequisite for federation across languages.
- `provekit-cli` `cmd_compose` — CLI surface for ad-hoc composition over a contract set.
- C lifter `effects.c` + `composition.c` — first-language wiring of the composition protocol against the C kit.

### Specimens

- **BZ-COMPOSITION-001.** First runnable bug-zoo specimen demonstrating PPP + CCP end-to-end. The pattern is named editorially, the predicate compiles to substrate SQL over the borrowed-pages-as-scratch v2 fact table, and the closure witness discharges through CCP-composed contracts. Empirical cross-check on the Linux net/ subtree.

### Posture

No core verifier behavior changes. No ProofIR grammar, canonicalization, proof-file format, or all-layer lift output semantics change. No cross-kit conformance fixture semantics change. Existing v1.6.3 mementos, fixtures, and `.proof` bundles remain valid byte-for-byte.

Patch label is correct under v1.6.3 bootstrap-policy's `versionLabelRule.extensionOnlyWithoutCrossKitSemanticObligation: patch`. Both new properties are added (not modified); zero existing property CIDs change.

## Verify chain (historical catalogs)

```
v1.4.0  blake3-512:b0f2030d56c2fddf...
v1.4.1  blake3-512:dc2f42ff8a4a6628... (bluepaper freeze, May 3)
v1.5.0  blake3-512:540e8c1f5f7fea88...
v1.6.0  blake3-512:ce04a4053498...     (FloatSort + RegionSort)
v1.6.1  blake3-512:fa1fbf90b7f0...
v1.6.2  blake3-512:52bdb2be4b38...
v1.6.3  blake3-512:dd0cc79889ee...
v1.6.4  blake3-512:09ccf7b14646...     <- this release
```

Each step is a witnessed PEP migration edge. The chain itself is the authority.

## First principle

*Supra omnia, rectum.* (T Savo)

---

**Tagging command (when ready to ship):**

```sh
git tag -s v1.6.4 -m "ProvekIt v1.6.4: protocol catalog 09ccf7b14646..."
git push origin v1.6.4
gh release create v1.6.4 --notes-file docs/launch/release-notes-v1.6.4.md
```
