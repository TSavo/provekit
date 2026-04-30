# ZK-attested verification + paid verification economy

> **Status: SIDE PROJECT. Deferred from v1 scope.**
>
> Captured 2026-04-29 during architectural session (T + Claude). Not on
> the v1 implementation path. Pick this up when v1 software-domain
> adoption locks in and the framework needs to extend to proprietary
> content + paid marketplace economics.

## The insight

**Zero-knowledge proofs absorb into the framework as a producer type.**

A ZK prover is just a producer whose evidence variant is
`zk-attested-verification`. Its output is a content-addressed memento.
The proof artifact is in the evidence body. The verifier (the algorithm
that checks the proof) is a downstream consumer that re-runs the ZK
verification.

The framework's architectural primitive doesn't change. The domain of
applicability widens by absorbing ZK as a producer type, the same way
hardware enclaves are absorbed as a witness producer type, the same way
standards bodies will be absorbed as spec-leaf producers.

## What this enables

**Verification of proprietary content without disclosure.**

Today, proof DAGs work for open-source code (no privacy needed).
Tomorrow, with ZK absorbed: the substrate works for any code. A bank
can prove their proprietary trading algorithm satisfies regulatory
invariants WITHOUT revealing the algorithm. A SaaS company can prove
GDPR compliance WITHOUT exposing source. An AI lab can prove "passes
safety eval S" WITHOUT releasing model weights.

The proof is content-addressed; the underlying content is private. The
ZK proof composes into the DAG; the secret stays secret.

## Use cases

- **Private code verification.** Proprietary algorithms prove
  compliance without source disclosure.
- **Trustless outsourced verification.** Pay a producer in another
  jurisdiction; they produce a ZK proof of work done; you verify the
  proof without revealing your code to them.
- **AI model evaluation.** Model author proves "passes safety eval S"
  without exposing weights.
- **Cross-jurisdiction compliance.** EU regulator requires GDPR; US
  producer proves compliance via ZK without revealing source to EU
  oversight.
- **Private supply-chain attestation.** Manufacturer proves "no
  contaminated ingredients" without revealing supplier list; auditor
  proves "audited" without revealing audit methodology.
- **Marketplace pricing privacy.** Verification payment is on-chain
  (public); the underlying code being verified is private (ZK-
  protected); the verdict composes globally.

## The marketplace economics

A paid verification economy forms naturally:

- Consumers need novel verifications (propertyHashes not yet in the
  global DAG).
- Producers compete on cost-per-novel-verification.
- Payment is content-addressed (Bitcoin tx hash, Lightning, on-chain
  attestation).
- The first paying consumer subsidizes everyone after them (free-rider
  problem).
- Once verified, the propertyHash is a public good.

Why anyone pays:
- First-mover need (waiting for someone else might be never)
- Compliance (regulators require verification; pay-or-fine)
- Reputation (shipping unverified software is costly)
- Royalty mechanisms (first payer's cost partly refunded by future
  consumers via on-chain logic)
- Network effects (richer DAG = competitive advantage)

ZK proofs unlock the proprietary segment of the market. Without ZK,
the verification economy works for open source. With ZK, it works for
the 95% of software value locked in proprietary enterprise systems.

## The marketplace splits by privacy

| Verification type | Cost | Volume | Privacy | Composition |
|---|---|---|---|---|
| Transparent (z3, formal methods, tests) | Low | High | Public | Trivial |
| ZK-attested (private content) | High (ZK proof gen is expensive) | Lower | Strong | Selective |
| Hardware-witnessed (TEE/enclave) | Medium | Medium | Strong-but-vendor-rooted | Trivial |

All three compose into the same DAG. A consumer's proofkit configures
which it accepts.

## What "doing the verification work" looks like as a memento

The verification work itself is content-addressable. Three flavors:

**Self-attestation.** Producer signs their own work in the evidence
body:

```yaml
kind: smt-proof
body:
  smtProgram: <hash>
  cpuTimeMs: 4521
  memoryMb: 312
  proofWitness: <z3 proof artifact>
  completedAt: <iso8601>
  workEnvironmentCid: <hash of producer's env>
producerSignature: <ed25519>
```

**Third-party witness.** Producer runs in a hardware enclave (Intel
SGX, AMD SEV, Apple Secure Enclave, AWS Nitro). The enclave signs:

```yaml
kind: enclave-witnessed-verification
body:
  producerCapability: "z3-symbolic@4.13.4"
  enclaveAttestation: <SGX/SEV/Nitro signed attestation>
  workTraceHash: <hash of the producer's runtime trace>
producerSignature: <z3's ed25519>
witnessSignature: <enclave's hardware signature>
```

**Payment attestation.** The consumer who paid signs:

```yaml
kind: verification-payment
body:
  producerCapability: "z3-symbolic@4.13.4"
  workMementoCid: <CID of the work memento>
  paymentAmount: 0.001 BTC
  paymentTxHash: <bitcoin transaction hash>
buyerSignature: <consumer's key>
```

## Why deferred

v1 scope is software correctness. The TS-IR language spec, the
canonicalizer, the kit catalog, the lifter, `provekit prove` and
`provekit generate` — all without ZK.

ZK absorption is downstream of v1 adoption. Reasons:

1. **v1 must work for open source first.** Open-source dominance
   creates the network-effect substrate; ZK extends it later. Trying
   to do both simultaneously dilutes focus.
2. **ZK proof generation is expensive.** Real-world ZK provers
   (Halo2, Plonky2, RISC Zero) take seconds to minutes per proof.
   Premature ZK integration would slow the v1 commit-gate experience
   to the point of unadoptability.
3. **The framework's architectural primitive doesn't depend on ZK.**
   Adding ZK is a producer-type extension; it doesn't require core
   redesign. We can defer cleanly.
4. **The marketplace economics need v1 adoption to work.** Without
   widespread v1 adoption, there's no "global proof DAG" for ZK-
   attested mementos to compose into. Build the substrate first;
   layer ZK on after.

## Pickup criteria

Revisit this when:

- v1 software adoption is locked in (TS kit catalog dominant in npm
  ecosystem; lockfile pinning includes proofHashes by default)
- A pilot enterprise customer has proprietary code requiring ZK-
  attested compliance (likely a Tier-1 bank; happens organically as
  pilots scale beyond the COBOL kit's open-source-friendly use case)
- ZK proof generation costs drop sufficiently (currently $0.01-$10
  per proof for non-trivial circuits; needs to be near-free for
  routine commit-gate use)
- A producer marketplace prototype shows demand (someone is willing
  to pay for novel verifications; payment infrastructure works)

## Reference points

- **Halo2** (Zcash Foundation) — recursion-friendly ZK proof system
- **Plonky2** (Polygon Zero) — STARK-based, fast proving
- **RISC Zero** — ZK virtual machine; runs general-purpose code under
  ZK proof
- **EZKL** — ZK proofs for ML model evaluation
- **Aleo / Aztec** — privacy-preserving smart contracts; the
  marketplace primitives we'd compose against

Bitcoin (transaction hashes for payment), IPFS (content-addressed
storage of ZK proof artifacts), and Lightning (micropayments for
small verification jobs) are the existing infrastructure we'd lean on
without reinventing.

## Architectural questions to answer at pickup

- Standardized ZK proof memento variant body schema (which fields are
  required, which are optional, which proof systems' proofs go where)
- Verifier-as-producer registration (each ZK proof system registers
  its verifier circuit's hash; consumer's proofkit knows which
  circuits it trusts)
- Payment-marketplace protocol (on-chain pricing? Off-chain bidding?
  Auction format?)
- Royalty mechanism for first-payer subsidy (smart contract that
  splits future consumers' payments back to the original payer)
- ZK + cross-language equivalence (does a ZK proof of property P
  about a TS function compose with the same propertyHash for an
  equivalent Rust function? Probably yes, but verifier circuits
  might need bridging)

## Status reminder

**This is a side project. Not v1 work. Pick up when the trigger
conditions above are met.**
