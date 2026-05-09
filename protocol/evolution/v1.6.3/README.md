# v1.6.3 Protocol Evolution Dogfood

This directory carries the PEP-shaped protocol evolution edge that
re-bakes the Lift Plugin Protocol to formalize identify-only package
inspection:

```text
v1.6.2 catalog
  -> v1.6.3 catalog
  -> ProtocolEvolutionBodyClaim
  -> TruthDischargeWitness(result = true)
```

The transition is patch-level under PEP because it is an
extension-surface clarification over the existing `provekit-lift/1`
wire protocol. It does not change core substrate verification, ProofIR
grammar, canonicalization, `.proof` format, cross-kit fixture semantics,
or proof-producing all-layer lift semantics.

## CIDs

| Artifact | CID |
|---|---|
| `from-catalog-v1.6.2.json` | `blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f` |
| `to-catalog-v1.6.3.json` | `blake3-512:dd0cc79889ee67d2594f5cfa20a191bafed15196fb2c5036f85deced7cd976055ae93825edebc10812b6fcf3c6ccf274fbc1137f32705aa0dc5938dc5825e31d` |
| Lift Plugin Protocol spec | `blake3-512:f2b856a8010b0f95cdd9961e0c367b003b1de7be39b6668db7f96cfe884a99f153609a846be39ad4a4f40a3bb778fecf2b0e24908b94411f32be165473045055` |
| `catalog-diff.json` | `blake3-512:f6b1f2e136996f5b53b54b838e291ce18ad6501ef7b93ec50bf9755ddc2a3b40a5b871ec1729ac37fbea372ad0b3afedd78a0724a7dfad14c781931a2ff31da1` |
| `bootstrap-policy.json` | `blake3-512:d84e27ae351bb151c10521e9c66d571b57e29035fa0248c7e18f68cefe8b45d7a9bbce5dc3dc9f95100ba02855059f75fd807cc6863add7c80bf3b028ee7abf5` |
| `bootstrap-verifier.json` | `blake3-512:8df69e305bbe380ff8e3a2a3996530201e59cb80f7c2417dbc01f051986490d4ffbf36ca85e52b5961e602c340b47e3551479010151fa023d76054db02524541` |
| v1.6.3 catalog attestation | `blake3-512:42e783a923f4b07ef86d414e66f7ecdf56f268c587c982b4abbf29e083e7fdbfcfe6869af709306da9a901247fae1b43a4cdd4c51c30a45759e017d9497b6ec3` |
| `protocol-evolution.body.json` | `blake3-512:bf7d3d9c5be556f7164caea22ec01681a64bddad481e564448a921abffdd86bb408b5e94f0680a257c2dbe02ed399cf0e4e2a1416a71a31215483f8908b34530` |
| `protocol-evolution.witness.json` | `blake3-512:8fd0ede538f9c7c43a2a9478a598bed2c9a2a7aa362d7636c9818546cdf6fd7b15fe8ca17f24c4760818bfc49e7d1c977ab9408b7d213f68d68390c3df2e8bb9` |

The witness is deliberately outside the body it discharges. Parent claims
that rely on this protocol transition should reference the witness root.

## Check

The already-built PEP body can be re-checked without regenerating the
witness:

```bash
provekit protocol check-evolution \
  --body protocol/evolution/v1.6.3/protocol-evolution.body.json \
  --from protocol/evolution/v1.6.3/from-catalog-v1.6.2.json \
  --to protocol/evolution/v1.6.3/to-catalog-v1.6.3.json \
  --policy protocol/evolution/v1.6.3/bootstrap-policy.json \
  --verifier protocol/evolution/v1.6.3/bootstrap-verifier.json \
  --catalog-diff protocol/evolution/v1.6.3/catalog-diff.json \
  --attestation .provekit/catalog-signatures/v1.6.3.json
```
