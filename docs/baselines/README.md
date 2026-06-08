# Foundation baseline catalogs

Per-language advisory catalogs of hidden predicates about each language's
standard library. These catalogs are signed by the Sugar foundation v0
ed25519 key as a starting point for users who want to verify proofs about
code in the named language.

**They are NOT authoritative.** The authoritative signer for any language's
contracts is the language steward, for example the rust-lang team for Rust,
the Go core team for Go, or Python core developers for Python. If a steward
has signed their own catalog, prefer it. If they have not, fork the foundation
baseline and sign your own; see
[`docs/contributing/signing-your-own-catalog.md`](../contributing/signing-your-own-catalog.md).

## Publication model

The canonical publication surface is content-addressed:

- Baseline proof files live at `.sugar/baselines/<proof_cid>.proof`.
- The active publication index lives at
  [`.sugar/baselines/blake3-512:2dd7d05a74fb96dc9d1b06aa6e261d2466b31fee927927456075d6d48adb2d43aed0cd0125c71d564cf01788f360bf0465342d97f5060ddc3c23379ac3383216.baseline-index.json`](../../.sugar/baselines/blake3-512:2dd7d05a74fb96dc9d1b06aa6e261d2466b31fee927927456075d6d48adb2d43aed0cd0125c71d564cf01788f360bf0465342d97f5060ddc3c23379ac3383216.baseline-index.json).
- The index CID is `blake3-512:2dd7d05a74fb96dc9d1b06aa6e261d2466b31fee927927456075d6d48adb2d43aed0cd0125c71d564cf01788f360bf0465342d97f5060ddc3c23379ac3383216`, computed as `BLAKE3-512(JCS(index-json))`.

Friendly named proof aliases such as `<lang>-baseline-v1.proof` are not a
canonical publication surface. Consumers pin the index CID and the per-language
`proof_cid` values inside it.

There are no `.proof.idx` sidecars and no filename-derived index compatibility
paths. LSP callsite lookup uses the content-addressed artifact defined in
[`docs/lsp/callsite-resolution-v1.md`](../lsp/callsite-resolution-v1.md); the
publication index records `compatibility_aliases: []` for that surface.

## How baselines are structured

Each baseline is a `.proof` envelope containing:

- Signed contract mementos for the named language's standard library surface.
- Optional disclaimer mementos where a baseline generator includes them in-band.
- Envelope metadata where available identifying signer role, language, language
  version, kit version, and disclaimer CID.

The compliance criteria a baseline must meet to ship at v1.0.0 are specified in
[`docs/contributing/baseline-catalog-rubric.md`](../contributing/baseline-catalog-rubric.md).

## Catalog index

| Language | Proof CID | Contract set CID | Contracts | Authored against | Status |
|----------|-----------|------------------|-----------|------------------|--------|
| c | `blake3-512:6a30a4529f801d877a4f94299dd51f13f6bbbd06b43ce6d41f82bd262b7c9bd1d5a777a5534c8b83991dffb0fa0ed8654f15df33272c98f1b9568b29c92ecd7e` | `blake3-512:1239777105ac988945bf7fd014ecfe1c543251502a0070923242fc9ceda514f949e0b8c0b734c971357e29a3cf9df7bc3ba2d0edd484b47b2b9986e0d7c82e37` | 22 | C11 | shipped |
| cpp | `blake3-512:64fc0845bff0143ce313e421ec503bd38afb6be6669f9fe80cdb940172ec20804bd1e9545423969b4920bd2ac4a3bbe34df0d4fcae62ea779489e484bb864938` | `blake3-512:001bca9f51ac4586d477a32be09b866fdfc11b2bd4f3a34c36c582a070f6b25d13cba48e885d6d433e826303a3d5eda8f6a08c79bf9cf3cb00649ab98c992a47` | 28 | C++20 | shipped |
| csharp | `blake3-512:e221722e5afc23d70e7ce4c06b23361e8725a4a2ae8477f77113747dfca974ac393b9ad805cdddd6b0533e61c7948052ea4a0b2a3f6e876766e41a616c217d39` | `blake3-512:741825c1ed622966e4476d3d322e1f6a2a747354cac2dc9a8e931c415ea57e49ffa82f1b8c61429d1bf34ade8b2ff77566b98f1ceec9a2004d8cd4e7eebef5d0` | 27 | 12.0 | shipped |
| go | `blake3-512:efa64a6c18cdd0bd8688f8814239c0bad6ca8018a7bad05c0f7cd5a708a5d2d3043ddc3c409ba03dc3b4d3acb9a43fbab4692e60e7da1b5c300112f7009dc54b` | `blake3-512:a961838071ef3907bc57988688652189a0636da0da901959c12e7c852a96fecefc14da3f9c73d1ea3fa2fc3cb5117ce12d74bc1e26f59112ee656b183f1ea304` | 23 | 1.22 | shipped |
| java | `blake3-512:22a411192a8988dee8bec3cfdb8a1508417310b5f16385395980c548c39abf739c061519b55681435b1149bded9db6e711eaaa0e93f0a100604fcd9e11943cf3` | `blake3-512:50e03b680fae6852aad60e8b28b00d605dfcdf434a61cad83a380420ab7a1044002465c1df650aeedcccb20c8a5ad18a92d73732508968960f5bafcf809c6876` | 27 | 21 | shipped |
| php | `blake3-512:93028afdee1ceb69c31114e189a9652cf0d029935278706448973e4a2356c23c2680238b115d4fe7df853611667e081d70ee006285bd26914302561b5e61cd3d` | `blake3-512:8ec70a3f080cc5667da3f4f347ad1b10ec2a6cde3666b300c3dc3c4f9a8ca8f595de8cd8bd71e8ee5bc13dfd1677f7f0d4014947f0b651c41dcbf77d235471ef` | 27 | 8.3 | shipped |
| python | `blake3-512:224e0cbe396f651a4a8d2675b987c7c849278439dd88b7eafa1f53c2b6e5ecfc0c6dcc9e23f59617ee1322fc0fd82759340d56ad417c2dd4d501683b2e510b17` | `blake3-512:8fc6df1ceb5466d3fc50f779098143910fa62714133ede5a36f7be4f7562ff44dc6d8a4d601d41393c7f9446113df93a1bdf2e33b9c33724d759d4d57b11bb1a` | 27 | 3.12 | shipped |
| ruby | `blake3-512:1c52edafb3bf42017062d4d8999ac80b9ed845b4e3bfa94e95021c47927b450daddaae43332323553ac9277c6d3336f8fba69aaf729ae41bce376ac869b62b4e` | `blake3-512:db682b4c7cf14217577871f3cc7bfe0d0963f6cf1c9435d614beb820fe12877eabfb8b8308f7a2739c9a4a3aec3d59612344129938e8f2a6fa15d7c49fd16b02` | 29 | 3.3 | shipped |
| rust | `blake3-512:60dc813e4af21119e328a889a7bffd9d89ce381ac7c328c08d5788cba7986754c8804f35bf5d5bdbaa450a56f979fe19ef407270acc5bfd7f027097cd98c14ec` | `blake3-512:76c278afe2f60f5b58ebaf53df1078143204cddbbf47d23119fb9778e17b6488004b3f8d8ca471720c1774483ea0fc7cb6dba0a097c638cc7e8231edb566d5e4` | 157 | rustc 1.81.0 | shipped |
| swift | `blake3-512:313ab922bf5c996dfe6075ec18e2483b072e14909a95e591778fdbdeff863c94dc1b917444be712cb9b6e07c6e54593174706efce51c9c9ff596ed0aeeac6244` | `blake3-512:0b57713351924b0803d114750f37fec8ea4a162800ec90f750d9ef4e3c9adf36dcbb80bef9ec1161cdbb1f8388044ecd625f0f04a04294635b6ed5af719a6286` | 28 | 5.10 | shipped |
| typescript | `blake3-512:9da2bdfffc4721f6cd66595435f99cc5fed98ba81407826fd0d4a8d25948f269e45e30a0e094b42564048487486e4596765da05386e964627e0fa90e80a3e85d` | `blake3-512:42a3d7eeaf0dbd76ae3bae34daa31e8281633e13e0589e2e027a44b4b8c6c5daa74345ddf8d99bdddbd267b3299d43e43b9deaace13f0ea8caf35edcbdaa67e7` | 44 | ES2022 | shipped |
| zig | `blake3-512:5f7e1f73c9a521d42ae5d77f602b96e328fb9e4f4b8efecae4f78b2d5b464a4ac45ca93a5eda69e485096f77b11bc018449d60c85d6ea23eec82135df8d80e55` | `blake3-512:112239c59ab57d246590836b63b40fb2c04183db3b36ecf280406783485011461d9d54019eb17e9f3997d823f58c81b45cf353f0598ffcbcf6f338f657e0129f` | 28 | 0.13 | shipped |

Each row is also present in the JSON publication index with `kit`,
`baseline_name`, `baseline_version`, `proof_path`, `member_count`, `signer`,
`signer_role`, `declared_at`, and `status`.

## Federation

The federated index of known signers lives at
`protocol/federation/known-signers.toml` when published. Stewards who sign their
own catalogs add an entry there pointing at their pubkey. Consumers pin signers
from the index, from elsewhere, or directly by hardcoded pubkey. The foundation
does not curate the index: listing means "self-declared standing," not
"foundation endorsement."

## Historical notes

Issue #477 supersedes the old v1.0.0 baseline launch issue text that assumed
friendly filenames and `.idx` siblings. Those launch issues are historical only;
the active acceptance contract is the content-addressed publication model above.

## See also

- [`docs/contributing/baseline-catalog-rubric.md`](../contributing/baseline-catalog-rubric.md)
- [`docs/contributing/signing-your-own-catalog.md`](../contributing/signing-your-own-catalog.md)
- [`docs/lsp/callsite-resolution-v1.md`](../lsp/callsite-resolution-v1.md)
