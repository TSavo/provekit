# Foo Algebraic Shape

This exhibit makes the cross-language federation concrete for:

```c
int foo(int x) {
  if (x == 0) return -22;
  return x;
}
```

The C, AArch64, and x86-64 lifts have different names, return slots, and value representations. Under a quotient that renames the input to `arg_0`, renames the return slot to `ret`, and interprets 32 bit machine literals as signed `Int`, all three collapse to one algebraic shape:

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
| c_foo | blake3-512:4b174d8546688510bc04c3c431c5e3802ccce0154c433249485d8f6c02c89ea04debda5defeb98abbb2749aacbee6c2a3c6d8e6216ab80666af5c7b474ea3f9d |
| aarch64_foo | blake3-512:da99cc9703b8a9819b472dfac9695efa4d3f2dd7c010837487cb59eb87cdc1e49460d9060daf317758e39f2bc1450950935d297720d5c7e70e1d3c1d90164a6b |
| x86_64_foo | blake3-512:3423071e79d49d1862b91f97f3f9c01890bb2d83f084344cf6d3ccbffe88aa5bc7d2552080fdc20cec005bb445b111a6ba91cd053527b9856ad919a35235e62a |
| morphism:morphism_c_to_shape | blake3-512:4d9d90222be14688553e82a400929aaca669999bc752620723d606172dce0edd6a37f78c736d6c02f90e19cd120247d1070230e80ca0323ae0d93e758e60f351 |
| receipt:morphism_c_to_shape | blake3-512:0f0ac21b546a9329973ef2b7c01fc941550e8962dbda84d844d8d354c085a8bd06dcef1e042b65c19eb41aa402e55c8144a0826d954ca39add2d8fa67724325f |
| morphism:morphism_aarch64_to_shape | blake3-512:f2fc80b9ea1fb1400dc38fcd1f4ee2d089f8ac281aa1f3b1f8bdec99f2ddb66880ef7c13ccf538f871bd0d31c064c070096c4c5a134a97c9bd972ced5121ac07 |
| receipt:morphism_aarch64_to_shape | blake3-512:860fffa606e4e323973132473ab3deadbab17df7160eb3b5e82e5ecddb94b20ea16419117d06e3e6e6ac400d40d30cef695161a8a6072f81dd0880757dfb3737 |
| morphism:morphism_x86_64_to_shape | blake3-512:af8fb003b25ea40a5ea8a9bc7e41c3b37c08265b03a20c5d00b3d41905487006f6440955749b4cd97c681594c9cb8911e3d2fcf9a0f2275f972463587f94569b |
| receipt:morphism_x86_64_to_shape | blake3-512:84d9e063d7293ea871b9abc1788c3b10d9546721898c830d88e9dc82b640e952ce4b38b1e66e8c7c203e0e749758c6ae480cd1fff2740881453c5dbf19b8ae40 |

The x86-64 source file also has a raw JSON CID of `blake3-512:7f45173ab9f877627d32172ee1af554bef1c30937d4ccaac3a3956f6088d18cfdd98935b10cc203f93febeb4d1f3e5e17c21239afd47577aa12e90acee7c97f1` because the emitted contract includes its own `cid` field. The source contract CID used by the morphism is the embedded memento CID, verified by recomputing the canonical CID after removing that field. The prompt cited a historical x86-64 CID of `blake3-512:d6e0c04222f724cdb63d61dcf64962921246dad629113b025b9fd3ea3963a36a57e49efcd6f657b856d5983eb7f2234d6a15fa5ca6af7d88bc78a4705646d291`; the current lifter emits the CID shown above.

## Quotient

The quotient maps source names and representations into the shared shape.

| Source | Renaming | Representation |
| --- | --- | --- |
| C | `x -> arg_0`, `result -> ret` | `i32 -> Int` |
| AArch64 | `w0 -> arg_0`, `w0_out -> ret` | `BitVector32 -> Int` |
| x86-64 | `edi -> arg_0`, `eax_post -> ret` | `BitVector -> Int`, `0xffffffea -> -22` |

The discharge is not an SMT proof. The script applies the renaming and representation map, folds the x86-64 two's-complement literal to `-22`, canonicalizes the resulting payload, and checks that the CID equals the shape CID.

## Discharges

| Morphism | After substitution CID | Shape CID |
| --- | --- | --- |
| morphism_c_to_shape | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 |
| morphism_aarch64_to_shape | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 |
| morphism_x86_64_to_shape | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 | blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1 |

All three after-substitution CIDs must equal the shape CID. The receipts live in `receipts/` and are also stored under `catalog/receipts/`.

## C Lifter Gap

This exhibit uses `menagerie/c11-language-signature/example/foo.expected-wp-contract.json` for the C source contract. The current C lifter output in `foo.contract.json` drops the branch and emits `result = x`; that branch-sensitivity gap is known and is being fixed separately.

## Reproduce

Run:

```sh
menagerie/foo-algebraic-shape/mint.sh
```

The script builds the Rust CLI and the two asm lifters, refreshes `sources/`, mints the shape and morphisms into `catalog/`, writes receipts, updates `cids.tsv`, and scans this exhibit for forbidden dash characters and the forbidden sign-off name.

## References

- `protocol/specs/2026-05-09-algorithm-memento-protocol.md`
- `protocol/specs/2026-05-09-language-signature-protocol.md`
- `docs/papers/03-substrate-not-blockchain.md`
- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`

T Savo
