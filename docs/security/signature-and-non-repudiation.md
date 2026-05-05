# Signature and non-repudiation

ProvekIt is a protocol for content-addressing formal verifications. Every memento (every contract, every implication, every bridge, every proof bundle) is signed. This doc explains exactly what the signature buys and what it does not.

## The signature scheme

Ed25519. Specifically:

- **Algorithm**: Ed25519 (RFC 8032).
- **Public key size**: 32 bytes.
- **Signature size**: 64 bytes.
- **Hash function**: SHA-512 (Ed25519 internal).
- **Determinism**: signatures are deterministic; signing the same message with the same key always produces the same 64 bytes.

Ed25519 was chosen because it is:

- **Fast.** Sign + verify are sub-millisecond.
- **Compact.** 32-byte keys, 64-byte signatures.
- **Side-channel resistant.** Ed25519 is constant-time by design.
- **Deterministic.** No nonce-reuse failures.
- **Widely deployed.** TLS 1.3, SSH, Sigstore, age, Tor, and many more use Ed25519.

The deterministic property is load-bearing for the protocol: two parties signing the same message with the same key produce identical bytes. This matters for cross-kit conformance (every kit minting self-contracts under the foundation key produces identical signatures).

## What the signature covers

For different memento kinds, the signature covers different bytes:

### Claim envelope (contract memento)

Signature is over `innerBytes`, the canonical IR bytes inside the envelope. See [`../contributing/writing-a-kit/03-claim-envelope.md`](../contributing/writing-a-kit/03-claim-envelope.md).

A valid signature attests: "the holder of this private key claims that the function (or scope) bound to this contract satisfies the canonical IR formula at this `contractCid`" (the content-only hash of the IR bytes; signer-independent per `2026-05-03-contract-cid-vs-attestation-cid.md`).

### Implication memento

Signature is over the implication assertion bytes. The implication memento says: "contract A (`contractCid` X) implies contract B (`contractCid` Y), with this evidence." Implication mementos reference contracts by their content-only `contractCid`, not by `attestationCid`, so the implication holds regardless of which signer attested either contract.

A valid signature attests: "the holder of this private key claims that the implication holds, witnessed by this evidence."

### Bridge memento

Signature is over the bridge declaration bytes. See [`../contributing/writing-a-kit/06-bridge-IR.md`](../contributing/writing-a-kit/06-bridge-IR.md).

A valid signature attests: "the holder of this private key binds this implementation symbol to this reference contract (referenced by its content-only `contractCid` per `2026-05-03-contract-cid-vs-attestation-cid.md` R3) via this implication."

### Proof bundle (top-level)

Signature is over the canonical CBOR bytes of the bundle minus the signature field. See [`../contributing/writing-a-kit/04-proof-envelope.md`](../contributing/writing-a-kit/04-proof-envelope.md).

A valid signature attests: "the holder of this private key endorses this entire bundle as a coherent set of contracts, bridges, and evidence."

## Non-repudiation: what it actually means

Non-repudiation is the property that the signer cannot later deny having signed a memento. With Ed25519:

- The signature was produced by someone holding the private key.
- The public key is in the memento; verification is offline.
- Without breaking Ed25519 (currently believed infeasible), no one but the holder of the private key can produce a valid signature.

So if a memento is in the wild with a valid signature against a developer's public key, that developer (or someone with their private key) signed it.

This is non-repudiation in the cryptographic sense: the cryptography doesn't let the signer deny.

## What non-repudiation does NOT provide

Non-repudiation provides:

✓ Cryptographic evidence the signer signed.

It does not provide:

✗ Evidence the signer's claim is true.
✗ Evidence the signer was authorized to sign.
✗ Evidence the signer wasn't coerced.
✗ Evidence the signer's key wasn't compromised.

These are the residue. The protocol gives you "this signer signed"; the protocol does not give you "the signed claim is true" or "this signer is to be trusted."

## Trust decisions are local

The protocol does not dictate which keys are trusted. The verifier decides:

```yaml
# provekit.config.yaml
trusted_keys:
  contract_signers:
    - "ed25519/MIIBIjANBgkqhkiG..."  # alice@example.com
    - "ed25519/MIIBIjANBgkqhkiG..."  # bob@example.com
    - "ed25519/...:foundation"       # foundation key for self-contracts

  prover_keys:
    - "ed25519/...:z3-foundation"
    - "ed25519/...:cbmc-foundation"

reject_unknown_signers: true
```

A memento signed by an untrusted key fails verification. Different verifiers can have different policies; one verifier might trust 3 keys, another might trust 30.

The trust set is operational. ProvekIt provides the substrate; you choose your trust set.

## Key management: out-of-protocol

The protocol does not manage signing keys. Concerns the protocol does NOT address:

- **Key generation.** Use a secure random source. Use libsodium, BoringSSL, or similar.
- **Key storage.** Hardware tokens, HSM, KMS, secure enclaves are recommended. The protocol works with any storage.
- **Key rotation.** Rotate periodically. Old signatures remain valid against the old public key; the protocol's monotonicity preserves them.
- **Key revocation.** When a key is compromised, distribute revocation info out of band. Verifiers' `trusted_keys` lists are updated.
- **Quorum signing.** N-of-M signing reduces single-key compromise risk. The protocol does not prescribe a specific scheme; it accepts multiple signatures.

Key-management practices are a security domain in their own right. ProvekIt provides hooks for trust decisions; key custody is up to the operator.

## Foundation keys

The protocol distinguishes between **identity keys** (held by signers, used to sign contract mementos) and **foundation keys** (well-known, used for cross-kit self-contracts conformance).

The foundation key for v1.x is a fixed Ed25519 keypair whose public key is part of the protocol catalog. The private key is published in the repository. Every kit's self-contracts mint uses the foundation key, producing identical signatures across all kits (because Ed25519 is deterministic given the key and the message).

This is intentional. The foundation key is not a secret; its purpose is to give every kit access to the same signing identity for the canonical self-contracts. If you can sign with the foundation key, you can verify what the foundation key signs; that's the point.

The foundation key MUST NOT be used to sign real contract mementos. Real contracts are signed with developer keys.

## Quorum signing (forwards-looking)

The protocol's data model supports multiple signatures per memento. A memento can have:

```
"signatures": [
  {"publicKey": "...", "signature": "..."},
  {"publicKey": "...", "signature": "..."},
  {"publicKey": "...", "signature": "..."}
]
```

A verifier configured for `min_signatures: 2` requires at least 2 valid signatures. This buys defense-in-depth against single-key compromise.

Most v1.x kits implement single-signature today. Multi-signature support is in flight; see the kit standard.

## Threshold signatures (forwards-looking)

A separate scheme: instead of N independent signatures, a single signature jointly produced by N parties (Edwards-curve threshold schemes like FROST). Threshold signatures look like single signatures but require N parties to produce.

The protocol has no opinion on threshold vs. multi-signature. Either works. Operational tradeoff: threshold signatures are smaller but require coordination; multi-signature is larger but more robust to one party going offline.

## The signature is the substrate's commitment

When a `.proof` ships, every memento is signed. The bundle is signed at the top level. Anyone with the public keys can verify that:

- The bundle is unmodified since signing.
- The signers' identities are the ones claimed.
- The structure (members, bridges, evidence) is authenticated.

This is the protocol's commitment to making formal verifications portable. The signature is what makes "I claim my code is correct, here's the verification" cryptographically distinguishable from "I claim my code is correct, trust me."

In a world where typical deployment is "trust me, I'm the package author," ProvekIt's substrate adds a layer of cryptographic accountability. The package author signs; the consumer verifies; the chain is auditable. Failures are attributable.

This is the smaller-than-correctness, larger-than-nothing claim of cryptographic protocols. It is exactly the claim ProvekIt makes.

## Read next

- [threat-model.md](threat-model.md): threat model the signatures defend against.
- [solver-trust.md](solver-trust.md): how prover signatures fit in.
- [adapter-trust.md](adapter-trust.md): adapter signatures and the trust chain.
- [`../contributing/writing-a-kit/03-claim-envelope.md`](../contributing/writing-a-kit/03-claim-envelope.md): how the signing actually works in code.
