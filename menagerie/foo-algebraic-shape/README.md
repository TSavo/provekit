# Foo Algebraic Shape

This exhibit makes the cross-language federation concrete for:

```c
int foo(int x) {
  if (x == 0) return -22;
  return x;
}
```

The C, Rust, AArch64, and x86-64 lifts have different names, return slots, and value representations. Under a quotient that renames the input to `arg_0`, renames the return slot to `ret`, and interprets 32 bit machine literals as signed `Int`, all four collapse to one algebraic shape:

```text
lambda arg_0. ite(arg_0 == 0, -22, arg_0)
```

The federation anchor is the shape CID:

```text
blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1
```

## Contracts And CIDs

| Artifact | CID |
| --- | --- |
| shape:foo | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 |
| c_foo | blake3-512:550ccd7feef6552c93fbc8b283602180de5295468ac33ee6bfca3e9193694435b88e0924ba8f85ee7376867244c291091f81118f50262d4cec34fc7d9abfb7c4 |
| rust_foo | blake3-512:c9c532518fbfeb2da9c006a78c812e1ba24cd7d14c9e0f7299e4b609493773abc63bc8dd8dd219361e07cbdf93210644ba125910ea6c4f772b8faca006d65e6a |
| aarch64_foo | blake3-512:8e5241a91d1e91d67ba97639b8c1e9b91ac865a6332e5fc74b850f860ee4649709091a019c3c7a0e1dc9657fc94bedfeab7413120991beec7a439076439d0dbf |
| x86_64_foo | blake3-512:d34500ea7d3c9698c9047d456115093fb1b858a6f1768fc7f3caa70e158a80e5e708e5e06ae4b6aa5be12fe1f8aee1500852de06eec1e27794de280913fa6b02 |
| morphism:morphism_c_to_shape | blake3-512:87888fbe98ffe9a78fe5fa83e4aa7d49ea330be70ff6f7dad13f079dbfb3d095c59670023f03584eebc23c539482e981333105a3317c03cf66c1de8a41b0737e |
| receipt:morphism_c_to_shape | blake3-512:82314098fa8c8e7ed854d90b37b3f13443c0689b986e0b9be0f6dee944eec37000e7d932425c86db1f0650f67390d5e802155dfd90b46255b9f514156f188f96 |
| morphism:morphism_rust_to_shape | blake3-512:930bbc7ead57475e33e89ddeefbf9ae387c1d91efda4525ec76413f663645ada4315444dfd3a8eb9d023e26b3890c2cc5a2f5f1de460ae84d1b19dbc512b7a04 |
| receipt:morphism_rust_to_shape | blake3-512:4490fa0fc695594bedfe13aae3e49597279de6f67587cf12f3cadb892a9537423bd5b0a2ed0543eb013396bbf80f80f458fcf512dd38bf7878e7621815711bf2 |
| morphism:morphism_aarch64_to_shape | blake3-512:7bfe874f39906148be421f6b36df38df904d985e282394421756814891887ddff74d3bbf85890b63ce68348d1163d14c476d7bdc9fe01eb80904d3783270c59c |
| receipt:morphism_aarch64_to_shape | blake3-512:eaebaee19a484f9f8af115b7aafe8cba31cbfde2fb946acebcf5d5caaa06e0025008f88a0060a4ffee3ad554dc002947b21d0d34102f0ce3f245df99d00d9644 |
| morphism:morphism_x86_64_to_shape | blake3-512:bf775b92b935539f113dca3aa02815a0992047fa2dbffa38bba42ddd291b6cd44017fe6d2ccabbff34b7107581817c1bc933a3e73f027635276ce7ee0b92a945 |
| receipt:morphism_x86_64_to_shape | blake3-512:96f3c4d6300150d34d8d18caea1dfbe923235b397bba9f9a7f81039fa118879556543c7b3f5c19bd5e2066abf43cde5142e87bb18a189cbef323c45387daa846 |

The x86-64 source file also has a raw JSON CID of `blake3-512:506e5a73bc426062f339775fed16f14712bd57d13b4896669b6378be1fa9540f61b2a8950b2e39bf8b03a14750e85ea02093c87e2b8004d3d47ee395d97cd5d6` because the emitted contract includes its own `cid` field. The source contract CID used by the morphism is the embedded memento CID, verified by recomputing the canonical CID after removing that field. The prompt cited a historical x86-64 CID of `blake3-512:d6e0c04222f724cdb63d61dcf64962921246dad629113b025b9fd3ea3963a36a57e49efcd6f657b856d5983eb7f2234d6a15fa5ca6af7d88bc78a4705646d291`; the current lifter emits the CID shown above.

## Quotient

The quotient maps source names and representations into the shared shape.

| Source | Renaming | Representation |
| --- | --- | --- |
| C | `x -> arg_0`, `result -> ret` | `i32 -> Int` |
| Rust | `x -> arg_0`, `result -> ret` | `I32 -> Int` |
| AArch64 | `w0 -> arg_0`, `w0_out -> ret` | `BitVector32 -> Int` |
| x86-64 | `edi -> arg_0`, `eax_post -> ret` | `BitVector -> Int`, `0xffffffea -> -22` |

The discharge is not an SMT proof. The script applies the renaming and representation map, folds the x86-64 two's-complement literal to `-22`, canonicalizes the resulting payload, and checks that the CID equals the shape CID.

## Discharges

| Morphism | After substitution CID | Shape CID |
| --- | --- | --- |
| morphism_c_to_shape | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 |
| morphism_rust_to_shape | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 |
| morphism_aarch64_to_shape | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 |
| morphism_x86_64_to_shape | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 |

All four after-substitution CIDs must equal the shape CID. The receipts live in `receipts/` and are also stored under `catalog/receipts/`.

## C Lifter Gap

This exhibit uses `menagerie/c11-language-signature/example/foo.expected-wp-contract.json` for the C source contract. The current C lifter output in `foo.contract.json` drops the branch and emits `result = x`; that branch-sensitivity gap is known and is being fixed separately.

## Reproduce

Run:

```sh
menagerie/foo-algebraic-shape/mint.sh
```

The script builds the Rust CLI, Rust walker, and the two asm lifters, refreshes `sources/`, mints the shape and morphisms into `catalog/`, writes receipts, updates `cids.tsv`, and scans this exhibit for forbidden dash characters and the forbidden sign-off name.

## References

- `protocol/specs/2026-05-09-algorithm-memento-protocol.md`
- `protocol/specs/2026-05-09-language-signature-protocol.md`
- `docs/papers/03-substrate-not-blockchain.md`
- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`

T Savo
