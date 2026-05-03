# Provenance

Cryptographic attestations of authorship over the architectural assembly described in `docs/launch/substrate-not-blockchain.md`, `docs/launch/the-pieces-on-the-table.md`, `docs/launch/path-to-default.md`, and the seven 2026-05-* specs in `protocol/specs/`.

The substrate is dogfooded for the meta-claim: the same content-addressing + signing primitives the protocol provides for end-user contracts are used here for the architect's claim of authorship. Three independent verification axes:

1. **Umbrella CID:** `blake3-512(JCS(<sorted contentCids>))` over the canonical content. Recomputable from the repo by anyone, byte-deterministic across machines per manifesto §11. See `v1/umbrella.json`.

2. **Ed25519 attestation:** signed memento under the architect's real Ed25519 key (NOT the publicly-known v0 foundation seed in `tools/foundation-keygen/`). The signed claim is in `v1/attestation.json`; the public key is in `v1/pubkey.txt`; the binding to the architect's established public identity is in `v1/identity-binding.txt`.

3. **Public time anchor:** the attestation's CID is anchored on Bitcoin via OP_RETURN (txid in `v1/anchor-bitcoin-txid.txt`) and additionally via OpenTimestamps (proof in `v1/attestation.json.ots`). Either anchor independently establishes time-of-existence; both together guard against single-chain failure.

Each axis is verifiable without trusting the others. The umbrella CID does not need the signature to be valid (it commits to bytes, not to a signer). The signature does not need the on-chain anchor to be valid (it commits to a key, not to time). The on-chain anchor does not need the umbrella to be valid (it commits to whatever CID was encoded, by whoever broadcast the transaction). Composition under §10 of the manifesto: each axis is its own attestation, the conjunction is the strongest claim.

## Verify

```sh
./provenance/v1/verify.sh
```

Three checks: umbrella CID recomputation, Ed25519 signature against pubkey.txt, OP_RETURN payload at the anchored txid (or OpenTimestamps proof). Skips checks gracefully when the underlying tool isn't installed; passes only when all available checks pass.

## Versioning

`v1/` is the first attestation. If the architectural assembly changes (e.g., a new spec lands and the architect wants to extend the authorship claim), a new `v2/` directory is created with a fresh umbrella CID and fresh attestation. v1 stays valid as a historical claim over the v1-state of the assembly. The `vN` directories never replace each other; they accrete.

## What this is and what it isn't

It IS: a public, immutable, independently-verifiable assertion that a named architect authored a specific assembly of public primitives at a specific time, signed under a key they control, anchored on Bitcoin.

It IS NOT: a patent, a license, or a legal claim. The code in this repository remains under its license (Apache-2.0 per the SPDX identifiers throughout the source). The provenance directory establishes attribution; it does not modify rights to use the code.

## Background

The architect's career has a recurring pattern: assemble public primitives into something load-bearing, watch others build on it without attribution. Two prior cases: content-addressable deduplication at age 18 in 1995 (predates rsync), Digital Confetti at age 21 in 1998 (two direct attribution chains into BitTorrent via Jed McCaleb's eDonkey2000 and Bram Cohen). Both undercredited.

This time the credit chain is on-chain. The architectural moves named in the umbrella's content (multi-dimensional address, rank-N pin, contractSetCid, envelope/header/body layering, three-axis pinning, derived bridges via linker, linker-daemon-protocol, Locus normative pin) are the architect's. The primitives beneath them are public. The assembly is the contribution.
