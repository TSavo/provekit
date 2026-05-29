# Spec CIDs

Every spec in ProvekIt is content-addressed by BLAKE3-512. Verify the local
install conforms to the protocol catalog embedded in the CLI via
`provekit verify-protocol`.

## Two hashing rules

Spec files use **raw-bytes** hashing: `BLAKE3-512(file_bytes)`.

The catalog itself uses **JCS-canonical** hashing: `BLAKE3-512(JCS(catalog-json))`. This is because the catalog is JSON; canonicalizing first means the CID survives whitespace / key-order edits.

The `tools/recompute-spec-cids/` crate implements both rules. The single command that verifies everything:

```sh
cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify
```

`--verify` reads every spec in raw bytes, hashes each, then reads the catalog, JCS-canonicalizes it, hashes that, and compares all values. Exit 0 iff every value matches.

## Pinned CIDs

| Document | CID |
|---|---|
| **Protocol catalog (v1.6.6, current CLI)** | `blake3-512:809ed1ebd538f206beb9df6de712f502fbcd310ee52d76c34afecec6455259d49cd7d288eb761d5aac9ebbd3643ae4dfe09bc9c7f2aea23e57720df6085c6640` |
| Protocol catalog (v1.6.5, historical) | `blake3-512:42ab046d530993a039cb6f78d8edb20b9e5f001f96182e57890379ccf9dbc9233430159724422ba4b91f783953f3e0ef3f8d56d4c112085904e8b08fbfce02d0` |
| Protocol catalog (v1.6.4, historical) | `blake3-512:09ccf7b1464622eceb4ac0e9bae3b435ba92d87c19e89f93724e6be75f4afce9eb3dedb7b8ebe2536de054143efefcb3cb622e6e5b4140bb26e6156a9bc9adf3` |
| Protocol catalog (v1.6.3, historical) | `blake3-512:dd0cc79889ee67d2594f5cfa20a191bafed15196fb2c5036f85deced7cd976055ae93825edebc10812b6fcf3c6ccf274fbc1137f32705aa0dc5938dc5825e31d` |
| Protocol catalog (v1.6.2, historical) | `blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f` |
| Protocol catalog (v1.6.1, historical) | `blake3-512:fa1fbf90b7f092b732cd2b088d12210befe304065acbe0f9640785a911dd917f1c49fb90d1ff4dcd1861310cf739350ef60b46f1b54be0ea54ccb09d0c1b76f0` |
| Protocol catalog (v1.6.0, historical) | `blake3-512:ce04a40534986a95362d5f130fd3a1a667b7a157f0554f262af11ec7a2ac8e8b80f56c36cca93d7a180535eedc99949d760fce6ab63c405de8837fa20f00e781` |
| Protocol catalog (v1.5.0, historical) | `blake3-512:540e8c1f5f7fea880123203b30891771d421da953c34af6bfb1d56d4c1d25dfb2ae08af6f275f5b4a4d015c364588b3521116541fcf4ac32d69b4e46acee1843` |
| Protocol catalog (v1.4.1, historical) | `blake3-512:dc2f42ff8a4a66289cc19bfbd628898b8bd8e61d2148ecf609324cc2421c5c440a6c0e70e20ffbecabeb78e0253101d72823b7e3ab120a4d56cb67c8e31dc641` |
| Protocol catalog (v1.4.0, historical) | `blake3-512:b0f2030d56c2fddf0ecbd7032bf0344c43e30677930e3b77188fcdc4ca6325d34649e51b2efa97d6985e4be6c43173f803254a7b05fc8bf31b92eb399b60f52f` |
| Canonicalization grammar | `blake3-512:4d8c2940c53a59c678c8fb65e33dc2cb0ae8ae8a283b97b9c69fd678565653d15e6ee9dc3ffc6a32dc1ff035821b0c1a006f0455498d2ea91faef845d7b39830` |
| Handshake algorithm | `blake3-512:acbf67dda9373c648e591d8ad74b8f8d56f4c92ba9c82bdc6690dc521e6f17012dd195e98a96b099090eeeb5a424312d90ff441c882d0e317a190561aa1a6925` |
| IR formal grammar (v1.6.0: FloatSort + RegionSort added) | `blake3-512:7b8f7dfaa7aee77fc8788d02b5a8f0aceca035ba8dc0c713348fd71787250c34f1f6038f26df1ca9c6a829d9bba79899582c8e8c71047227310f2e7e034908b2` |
| Lattice tractability theorem | `blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07` |
| Memento envelope grammar | `blake3-512:8b6b9d9ccb7091cfa9d0993ac1d9b02057a6f7c5d9e36a05849630b8b02887073fad44114c5025740bb458e58741e41e24c438a041e1dde6e903ace7bd48278e` |
| Proof file format | `blake3-512:a78f21484f8a55dbc0e3647433da95475820bad6f0db10643625315b030f7f114aec8710ea8cb3a4c3bfb096bf635487a31c86bec8978b90d0b4238b5eb6d266` |
| Self-contracts (stable; v1.1.0+) | `blake3-512:a0f58941758d709739759cf166bf9cb73794958144e213eccfb28fbf5791ca824ce53da0c6ba801cca2b53400324a094f510d4bbc41bc6b73b17e486ad3838ab` |
| Signatures and non-repudiation | `blake3-512:8b71229fcb7413f18a93a9b260012298311c1ce754850ee717780c181f1fda39a6600b2e5069e775cd7dd15e8c81e40b47bf7585aa0b23ab76c112c85116365c` |
| Substrate layers (envelope/header/body) | `blake3-512:a76c7d0d6a5fecde40f579a8566f0199f61d5e3f7a0d48561f0ef765b5983a31fc96bb21fe56ea667337341dc798e7370229722ee7535fe7e2c22d4f7dad97dd` |
| Contract CID vs attestation CID | `blake3-512:53e136d9af29ce80b690f90c484e90c60f66f28c483b2038e03e7c6f6055f637527deb205e5558b47487b7d89ead461348d5e8981f2e9e8ccb30edd8867d47db` |
| Contract set extension | `blake3-512:839e82096d04b1241ffa1f6158fcea6bfeb78f3836664a66a13ff11b3cde58d72e6c85bfc619ba1341f13b8375f655bdf5582b0eac91d27648f0048bee8f9867` |
| Version chains pinning | `blake3-512:281bf014f6f0ebc9a5d455329ee033ff8b7ee85e001bcbdcb45a62c14e43855892c46462789ccb74961859e708eae70829fdf736798c17f59f0239ef78dd7e45` |
| Protocol Evolution Protocol (PEP) | `blake3-512:d8827f89df20e5be38c4d5de851fe4e55420dcd6cacfd9b98f458c53e64e6ba07349e29f8da2fbab6cb7195b297c3704a70f489c020e3f55c96ef702c4a09949` |
| Lift Plugin Protocol | `blake3-512:f2b856a8010b0f95cdd9961e0c367b003b1de7be39b6668db7f96cfe884a99f153609a846be39ad4a4f40a3bb778fecf2b0e24908b94411f32be165473045055` |

The detailed property list below was originally written for v1.6.3 and still
records the stable core CIDs used by the current catalog. The current CLI's
authoritative answer is the embedded catalog checked by `provekit verify-protocol`;
historical signatures live under `.provekit/catalog-signatures/`.

## v1.6.3 changes (patch over v1.6.2)

One extension-surface re-bake; no core verifier, ProofIR grammar, canonicalization, proof-file format, or cross-language fixture semantic obligation changed. v1.6.2 mementos, fixtures, `.proof` bundles, and kit conformance obligations remain valid forever against the bytes they were minted for.

- **`lift-plugin-protocol`** formalizes `options.layer = "identify-only"` and the `package-inspection-document` result.
- `provekit package inspect` is specified as a client command over `pep/1.7.0`: it dispatches to the configured lifter and requires a `package-inspection-document`.
- Package inspection now has named rails for package artifact bytes, CI input closure, release contract sets, conventional receipts, admission hints, and shipped `.proof` files.
- The PEP transition is recorded under [`../../protocol/evolution/v1.6.3/`](../../protocol/evolution/v1.6.3/), with a `ProtocolEvolutionBodyClaim` and TDP-shaped witness.

## v1.6.1 changes (patch over v1.6.0)

One cataloged extension-only addition; no core verifier, ProofIR grammar, canonicalization, proof-file, or cross-language semantic obligation changed. v1.6.0 mementos, fixtures, and `.proof` bundles remain valid forever against the bytes they were minted for.

- **`protocol-evolution-protocol`** added as a draft extension protocol for signed, content-addressed protocol catalog transitions.
- PEP dogfoods the version-label policy: extension-only catalog additions with no required language-kit emission, lift, canonicalization, or verifier changes may be patch-level transitions.
- The transition is recorded under [`../../protocol/evolution/v1.6.1/`](../../protocol/evolution/v1.6.1/), with a `ProtocolEvolutionBodyClaim` and TDP-shaped witness.

## v1.6.0 changes (minor bump over v1.5.0)

One property re-bake; no breaking changes. v1.5.0 mementos and `.proof` bundles remain valid forever against the bytes they were minted for.

- **`ir-formal-grammar`** re-baked after the sort grammar grow in PR #401: `RegionSort` (kind=`region`, locked key order `kind/name`, `name` is a Rust lifetime identifier like `'a`, `'static`, or `'r0`) added to the IR sort algebra as a carrier for borrow-checker lifetime variables. The Sort union is now `PrimitiveSort | BitvecSort | SetSort | TupleSort | FunctionSort | DependentSort | FloatSort | RegionSort`. Formal invariants `RegionSort.ValidName` and `RegionSort.OpaqueToBackends` added.
- **`ir-formal-grammar`** also absorbs `FloatSort` (kind=`float`, locked key order `kind/width`, width in {16, 32, 64, 128}), which was added to the Rust enum in PR #389 but was omitted from the spec prose. Formal invariant `FloatSort.ValidWidth` added.

This is a schema-additive bump: contracts using pre-v1.6 sort variants continue to validate without modification. RegionSort is a prerequisite for #384 C.9 (Outlives predicates). Cross-kit conformance gate may go red while the 11 non-Rust kits add the RegionSort variant; that is expected and tracked as followup (mirrors the noted-followup from PR #361 / #389).

## v1.5.0 changes (minor bump over v1.4.1)

One property re-bake; no breaking changes. v1.4.1 mementos and `.proof` bundles remain valid forever against the bytes they were minted for.

- **`ir-formal-grammar`** re-baked after the sort grammar grow in PR #361: `FunctionSort` (kind=`function`, locked key order `kind/args/return`, `args` non-empty) and `DependentSort` (kind=`dependent`, locked key order `kind/name/indexVar/indexSort`) added to the IR sort algebra. The Sort union is now `PrimitiveSort | BitvecSort | SetSort | TupleSort | FunctionSort | DependentSort`. Both variants support recursive Sort references. Formal invariants `FunctionSort.ValidArgsAndReturn` and `DependentSort.ValidFields` added.

This is a schema-additive bump: contracts using only the pre-v1.5 sort variants continue to validate without modification. The minor version bump reflects the grammar change per the catalog versioning policy (additive grammar change = minor, not patch).

## v1.4.1 changes (patch over v1.4.0)

Three property re-bakes, no protocol-level breaking changes. v1.4.0 mementos and `.proof` bundles remain valid forever against the bytes they were minted for.

- **`ir-formal-grammar`** re-baked after the Locus type addition (closes a gap surfaced by PR #119, where the Go agent's call-edge emission inferred Locus shape from first principles rather than pulling from spec).
- **`proof-file-format`** re-baked after adding §6.1 specifying the v1.4 substrate-layers compatibility cut for catalog mementos and pinning the catalog `header.cid` recipe as `BLAKE3-512(JCS(sorted_member_cids))` (the same recipe as `contractSetCid` generalized to any member-set).
- **`memento-envelope-grammar`** re-baked after adding a supersession note pointing forward to `2026-05-03-substrate-layers-envelope-header-body.md` for v1.4-and-later mementos. The v1.1 flat shape remains valid for historical mementos under monotonicity.

One non-cataloged clarification: `2026-05-03-bridge-linkage-protocol.md` §1 DerivedBridge `schemaVersion` corrected from `"2"` to `"1"` consistent with substrate-layers-envelope-header-body's v1.4 layered-shape conventions and bridge-target-dimensionality §1 R3. This spec is not in the catalog `properties` map so no further property re-bake.

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

The full list of current spec CIDs is in `protocol/specs/2026-04-30-protocol-catalog.json`. Recompute locally to verify.

## Per-kit self-contract attestations

Each conformant peer ships hand-written contracts about its own public surface, mints them as signed mementos under the foundation key, and signs an external attestation under `.provekit/self-contracts-attestations/`.

The live attestation files, not this page, are the source of truth for per-kit bundle and `contractSetCid` values. Verify them with:

```sh
make conformance
```

Two runs producing the same CID is the framework verifying its own canonicalization is deterministic. If a local mint does not match its checked-in attestation, your bytes are not the bytes this protocol version was published against.

## Bluepaper recursive-verification

The protocol catalog's CID is the protocol version. Verifying the catalog is the
act of running the protocol; running the protocol verifies the catalog. There is
no external authority. The bluepaper at
[`../papers/02-bluepaper.md`](../papers/02-bluepaper.md) closes with this
recursive verification recipe. Run `--verify`. If the computed catalog CID
matches one of the current or historical catalog CIDs listed above, the
bluepaper has just verified its own authority over the bytes you have.

## Read next

- [`../papers/02-bluepaper.md`](../papers/02-bluepaper.md): full formal protocol specification with all spec CIDs.
- [`../explanation/architecture.md`](../explanation/architecture.md): protocol mechanics.
- [`../contributing/build.md`](../contributing/build.md): how to recompute via `make conformance`.
