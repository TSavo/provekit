# v1.6.1 Protocol Evolution Dogfood

This directory carries the first PEP-shaped protocol evolution edge:

```text
v1.6.0 catalog
  -> v1.6.1 catalog
  -> ProtocolEvolutionBodyClaim
  -> TruthDischargeWitness(result = true)
```

The transition is patch-level under PEP because it is `extension-only`
and does not create a new cross-kit semantic obligation.

## CIDs

| Artifact | CID |
|---|---|
| `from-catalog-v1.6.0.json` | `blake3-512:ce04a40534986a95362d5f130fd3a1a667b7a157f0554f262af11ec7a2ac8e8b80f56c36cca93d7a180535eedc99949d760fce6ab63c405de8837fa20f00e781` |
| `to-catalog-v1.6.1.json` | `blake3-512:fa1fbf90b7f092b732cd2b088d12210befe304065acbe0f9640785a911dd917f1c49fb90d1ff4dcd1861310cf739350ef60b46f1b54be0ea54ccb09d0c1b76f0` |
| PEP spec | `blake3-512:d8827f89df20e5be38c4d5de851fe4e55420dcd6cacfd9b98f458c53e64e6ba07349e29f8da2fbab6cb7195b297c3704a70f489c020e3f55c96ef702c4a09949` |
| `catalog-diff.json` | `blake3-512:3498eac3357cfaa327006a12723a2b021cdb9137d9b46ecb530d1de27c227d56250d68c5f318a68200b3ff3a28969124d48a78e52de2171fcadd06fe4794670a` |
| `bootstrap-policy.json` | `blake3-512:89aa938403f57caaef324246fb47d94b4a5c987a1802f7752fdf3412dce85bbed2d719d416444adce8983d25403b35513309b7a633b2eb781757c8eb82995e1c` |
| `bootstrap-verifier.json` | `blake3-512:bfb83e8738950f42cacfbad70529016699fa57db0ab33a7d0c629fa2e830dc853a69b603ff9745e6d8a1205514245dcaa26500de316e7cf971469747d6dd9489` |
| v1.6.1 catalog attestation | `blake3-512:8acc629d98a1418fff475f6b9a92bd32536989c64927e1ae1b1cce74dbfa182f9480ee516f3b9c6ffb97896b0cc419d65674ff19a9aab4832b52a09f6e48b210` |
| `protocol-evolution.body.json` | `blake3-512:bb70d4f03386bcab3d17fed004992e0530b4f58163f0a944759aa331445f0bf29181f889af77bfb22ade6c04f45ec7d6c9646b9f83815d10a4c8634206af3cde` |
| `protocol-evolution.witness.json` | `blake3-512:62524e7f98a56539aeb2684a9da0dbc764ead475a4306e2d50c30bac3cda3e3dd7166a709a34b2d7dadd952c1ad05997e16b3711d3006e3b4fe54cd83e461ce3` |

The witness is deliberately outside the body it discharges. Parent claims
that rely on this protocol transition should reference the witness root.

## Check

The already-built PEP body can be re-checked without regenerating the
witness:

```bash
sugar protocol check-evolution \
  --body protocol/evolution/v1.6.1/protocol-evolution.body.json \
  --from protocol/evolution/v1.6.1/from-catalog-v1.6.0.json \
  --to protocol/evolution/v1.6.1/to-catalog-v1.6.1.json \
  --policy protocol/evolution/v1.6.1/bootstrap-policy.json \
  --verifier protocol/evolution/v1.6.1/bootstrap-verifier.json \
  --catalog-diff protocol/evolution/v1.6.1/catalog-diff.json \
  --attestation .sugar/catalog-signatures/v1.6.1.json
```
