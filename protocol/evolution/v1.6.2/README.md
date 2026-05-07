# v1.6.2 Protocol Evolution Dogfood

This directory carries the PEP-shaped protocol evolution edge that
catalogs the Content-Addressed CI Protocol:

```text
v1.6.1 catalog
  -> v1.6.2 catalog
  -> ProtocolEvolutionBodyClaim
  -> TruthDischargeWitness(result = true)
```

The transition is patch-level under PEP because it is `extension-only`
and does not create a new cross-kit semantic obligation.

## CIDs

| Artifact | CID |
|---|---|
| `from-catalog-v1.6.1.json` | `blake3-512:fa1fbf90b7f092b732cd2b088d12210befe304065acbe0f9640785a911dd917f1c49fb90d1ff4dcd1861310cf739350ef60b46f1b54be0ea54ccb09d0c1b76f0` |
| `to-catalog-v1.6.2.json` | `blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f` |
| CICP spec | `blake3-512:4b63e8c58d59b54272407b624b67578b7e1a8fdeb71d41c7d5e18d3bd6d668e7f77c8e2b9a68a10d3732dda40baf66db27f87ab10cbdb1d52e857bcbb7d3ec47` |
| `catalog-diff.json` | `blake3-512:3463c8cd153d262eec88e117c242ca1024efc60595bfe37898e0cadd99c5408e4f0b24c1a256e83993e019992bb60944b111893438f986f3e6c85a504eade1f5` |
| `bootstrap-policy.json` | `blake3-512:27e35c0e356d97aadd51589e054a25db392e1db7d6f10d3fc3c9452de2161c364e3530e086a44b65de48f5115200d43f19c90ec068c0fa8b36128f2946c58274` |
| `bootstrap-verifier.json` | `blake3-512:fb1e040cb4e11ab9693566b081337b3dda7087d6ca41beaf7509b2a7ec72f41fad03db312e41fe672fc555799279f47163e7c4a9bd4e53a186a72f69ffa5489f` |
| v1.6.2 catalog attestation | `blake3-512:16d62ae58d3d3d14019f24b19d4c32d17e4e47c67d537d7da469c7e25acdd64b1d2e215a4dcb9b28b723d36b1afaee08062d1b50792f3c5fdb43d2778d9ad6b3` |
| `protocol-evolution.body.json` | `blake3-512:74086e48f2cbaa3bb3dcee4fab1411c131fba1b637620d63983bff15eb15144455e7bfc94240631d16b10247a05b6c7ad2390160c3c7bcdd973d4eab8d098e0b` |
| `protocol-evolution.witness.json` | `blake3-512:284f10c5a1572caa149701c45a82c439c10532fd4b59d076fdf1c42c7ff666387219c08b25457b14b8e642a9f97faabb007c4e4338ebac03eb8a679899f340b5` |

The witness is deliberately outside the body it discharges. Parent claims
that rely on this protocol transition should reference the witness root.

## Check

The already-built PEP body can be re-checked without regenerating the
witness:

```bash
provekit protocol check-evolution \
  --body protocol/evolution/v1.6.2/protocol-evolution.body.json \
  --from protocol/evolution/v1.6.2/from-catalog-v1.6.1.json \
  --to protocol/evolution/v1.6.2/to-catalog-v1.6.2.json \
  --policy protocol/evolution/v1.6.2/bootstrap-policy.json \
  --verifier protocol/evolution/v1.6.2/bootstrap-verifier.json \
  --catalog-diff protocol/evolution/v1.6.2/catalog-diff.json \
  --attestation .provekit/catalog-signatures/v1.6.2.json
```
