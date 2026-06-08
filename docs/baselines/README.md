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
| c | `blake3-512:85dfca1c1bc03425048a4f8537739cd77e3ba9bd7a7be1613d1266cef35a3d14d66a21d1cb2dd8c0cd1d856687c7fee90e4eac49dd6ae7ff2ffca0c04e3ec748` | `blake3-512:1239777105ac988945bf7fd014ecfe1c543251502a0070923242fc9ceda514f949e0b8c0b734c971357e29a3cf9df7bc3ba2d0edd484b47b2b9986e0d7c82e37` | 22 | C11 | shipped |
| cpp | `blake3-512:3dffca42c0eda0f7d445040e990cf455d63fb29246e7b70436fd80d8cc5649980ff0217c8cd3b7482814698a264e4c76f5973e4f5240e3670944f4a9914298e7` | `blake3-512:001bca9f51ac4586d477a32be09b866fdfc11b2bd4f3a34c36c582a070f6b25d13cba48e885d6d433e826303a3d5eda8f6a08c79bf9cf3cb00649ab98c992a47` | 28 | C++20 | shipped |
| csharp | `blake3-512:458546af9a403b10ab5836b797c340ae5281283e3efbb7b076c43ad607fe22a2e2e9c3223c3e116c40f014b6cf420f4ae8ba0f16594d0bf7ac6842942893dddd` | `blake3-512:741825c1ed622966e4476d3d322e1f6a2a747354cac2dc9a8e931c415ea57e49ffa82f1b8c61429d1bf34ade8b2ff77566b98f1ceec9a2004d8cd4e7eebef5d0` | 27 | 12.0 | shipped |
| go | `blake3-512:ce913e25a04b2c126b37470fda697edc7740340ecd2d1ab5c01091101b6e5af8819f930af80709d33faf49999f65087bf50ff35f7f6074b8fb3436b534f45e1a` | `blake3-512:a961838071ef3907bc57988688652189a0636da0da901959c12e7c852a96fecefc14da3f9c73d1ea3fa2fc3cb5117ce12d74bc1e26f59112ee656b183f1ea304` | 23 | 1.22 | shipped |
| java | `blake3-512:9629c444d57efeeab50bfe6b993c96cc34dd6c45ae87d72dda28395960dab88c523ce6b96e990fe56a9a5055b9a346d67605f78553481bb5fdd9395c5c40bf6c` | `blake3-512:50e03b680fae6852aad60e8b28b00d605dfcdf434a61cad83a380420ab7a1044002465c1df650aeedcccb20c8a5ad18a92d73732508968960f5bafcf809c6876` | 27 | 21 | shipped |
| php | `blake3-512:0bdc41b32a7aa5cb96ab6a0fb48bc6262c173a14d2c69350055c1041a031f6c764925d65197b82096bb0adc225b6ea06cf1b67ca9beed6f82717c3cd4e4de67b` | `blake3-512:8ec70a3f080cc5667da3f4f347ad1b10ec2a6cde3666b300c3dc3c4f9a8ca8f595de8cd8bd71e8ee5bc13dfd1677f7f0d4014947f0b651c41dcbf77d235471ef` | 27 | 8.3 | shipped |
| python | `blake3-512:772af80ca23efc3549ad925f2e6752502259d22b70d1e9a18489d501cefdd4cc703bbb12a673f9959c1d758ae5dd301dca04aa512c1fd17fc22b817e340e83d5` | `blake3-512:8fc6df1ceb5466d3fc50f779098143910fa62714133ede5a36f7be4f7562ff44dc6d8a4d601d41393c7f9446113df93a1bdf2e33b9c33724d759d4d57b11bb1a` | 27 | 3.12 | shipped |
| ruby | `blake3-512:24367457f1a21800bfa79359d0c1c86f843e956a875a6aa30b1f59a5c6021b9e7b15ba5fb4df1d183fbb43bc96af5a842cc21921b75050a40aa663180bf3a9f1` | `blake3-512:db682b4c7cf14217577871f3cc7bfe0d0963f6cf1c9435d614beb820fe12877eabfb8b8308f7a2739c9a4a3aec3d59612344129938e8f2a6fa15d7c49fd16b02` | 29 | 3.3 | shipped |
| rust | `blake3-512:a9dd90b55fbb89531d8aee8e443b805c53b910cb2dbceb1c8854ac71074fc89a485c38896838e94406f7fe5519903998d0f246b1c780127a7eaf38af7e621ce9` | `blake3-512:76c278afe2f60f5b58ebaf53df1078143204cddbbf47d23119fb9778e17b6488004b3f8d8ca471720c1774483ea0fc7cb6dba0a097c638cc7e8231edb566d5e4` | 157 | rustc 1.81.0 | shipped |
| swift | `blake3-512:d61ad4f0bc20dfebb68fe3df0d4fa452f9d8c0ca1511850f79b87ef384c66d9b566eda7988b252bee0137624d787954a1cd5d68260d16fb9a2437eb8b3a222a5` | `blake3-512:0b57713351924b0803d114750f37fec8ea4a162800ec90f750d9ef4e3c9adf36dcbb80bef9ec1161cdbb1f8388044ecd625f0f04a04294635b6ed5af719a6286` | 28 | 5.10 | shipped |
| typescript | `blake3-512:85f1c1f0ad79a1938dad674d54aa19f1d2e5c56f780ba3e427b3b1d74f3c12b9e70d0e431aee53ea0bb33019c0dbda8c7e7a617fad5d234970c13903265e57df` | `blake3-512:42a3d7eeaf0dbd76ae3bae34daa31e8281633e13e0589e2e027a44b4b8c6c5daa74345ddf8d99bdddbd267b3299d43e43b9deaace13f0ea8caf35edcbdaa67e7` | 44 | ES2022 | shipped |
| zig | `blake3-512:3744b53d9f392ff81e2efedc7a9ddb2907ce86707e026506295dd292b4d9a97913f153d541d32492b26edcf19a768b3e9a1f011590eb3ae476fb0e83a25db8c5` | `blake3-512:112239c59ab57d246590836b63b40fb2c04183db3b36ecf280406783485011461d9d54019eb17e9f3997d823f58c81b45cf353f0598ffcbcf6f338f657e0129f` | 28 | 0.13 | shipped |

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
