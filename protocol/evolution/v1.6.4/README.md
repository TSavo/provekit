# v1.6.4 Protocol Evolution Dogfood

This directory carries the PEP-shaped protocol evolution edge that
catalogs two new draft extension protocols, Pattern Predicate Protocol
(PPP) and Contract Composition Protocol (CCP):

```text
v1.6.3 catalog
  -> v1.6.4 catalog
  -> ProtocolEvolutionBodyClaim
  -> TruthDischargeWitness(result = true)
```

The transition is patch-level under PEP because both additions are
extension-only over the existing core substrate. They do not change
core verification, ProofIR grammar, canonicalization, `.proof` format,
cross-kit fixture semantics, or proof-producing all-layer lift
semantics. Existing v1.6.3 mementos, fixtures, `.proof` bundles, and
kit conformance obligations remain valid byte-for-byte.

PPP names how an editorially-defined bug class compiles to a
content-addressed substrate query whose result-set delta discharges an
FRP receipt's policy. CCP names canonical contract composition over the
existing handshake tier 2 cache and its FFI / CLI / direct-link binding
modes. PPP additively extends its v1 substrate schema with `effects`
and `composed_contracts` relations populated by CCP.

## CIDs

| Artifact | CID |
|---|---|
| `from-catalog-v1.6.3.json` | `blake3-512:dd0cc79889ee67d2594f5cfa20a191bafed15196fb2c5036f85deced7cd976055ae93825edebc10812b6fcf3c6ccf274fbc1137f32705aa0dc5938dc5825e31d` |
| `to-catalog-v1.6.4.json` | `blake3-512:09ccf7b1464622eceb4ac0e9bae3b435ba92d87c19e89f93724e6be75f4afce9eb3dedb7b8ebe2536de054143efefcb3cb622e6e5b4140bb26e6156a9bc9adf3` |
| Pattern Predicate Protocol spec | `blake3-512:d2758850fc4473d68150232e10ec27ff12382cc29881bacc0cd228767aa453a2246eb62022641e56407bfe83266a3619dc35ba7e03c41972f31365c738aea237` |
| Contract Composition Protocol spec | `blake3-512:632c68631e21bb9cd46b3bf347422330b5bfefd9011c48b18a9af9fb701f0b4f6dab8ddc10557aeb3a19c00f0a20efa5102d966273ad34d2445ce7413333b949` |
| `catalog-diff.json` | `<filled by provekit protocol evolve>` |
| `bootstrap-policy.json` | `<filled by provekit protocol evolve>` |
| `bootstrap-verifier.json` | `<filled by provekit protocol evolve>` |
| v1.6.4 catalog attestation | `<filled by sign-catalog-v1-6-4>` |
| `protocol-evolution.body.json` | `<filled by provekit protocol evolve>` |
| `protocol-evolution.witness.json` | `<filled by provekit protocol evolve>` |

The witness is deliberately outside the body it discharges. Parent claims
that rely on this protocol transition should reference the witness root.

## Check

The already-built PEP body can be re-checked without regenerating the
witness:

```bash
provekit protocol check-evolution \
  --body protocol/evolution/v1.6.4/protocol-evolution.body.json \
  --from protocol/evolution/v1.6.4/from-catalog-v1.6.3.json \
  --to protocol/evolution/v1.6.4/to-catalog-v1.6.4.json \
  --policy protocol/evolution/v1.6.4/bootstrap-policy.json \
  --verifier protocol/evolution/v1.6.4/bootstrap-verifier.json \
  --catalog-diff protocol/evolution/v1.6.4/catalog-diff.json \
  --attestation .provekit/catalog-signatures/v1.6.4.json
```
