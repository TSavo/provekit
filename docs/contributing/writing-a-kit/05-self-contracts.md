# Writing a kit, step 5: the self-contracts package

The self-contracts package is the conformance gate's strongest check. Every kit mints a fixed self-contracts catalog under the foundation key. The minted bundle's CID is pinned in `make conformance`. A single byte of drift in any kit component (canonicalizer, claim envelope codec, proof envelope codec) causes the self-contracts CID to drift, which the harness detects.

This is the strongest test because it composes every layer your kit has built. Steps 1-4 ensure individual fixtures pass; step 5 ensures the *whole stack composed* produces identical output.

## What self-contracts contains

The self-contracts bundle is a fixed `.proof` file containing a canonical set of contract mementos. The Rust kit defines the canonical content; every other kit must produce the same content byte-for-byte.

The exact contents are pinned in the protocol catalog. They include (at minimum):

- A small set of canonical Term and Formula instances exercising each IR primitive.
- A canonical Sort declaration.
- A canonical contract declaration (pre/post pair).
- A canonical bridge declaration (the v1.1.0 9-field shape).
- Foundation key public key and signature over each member.

Every kit writes a `mint-self-contracts` script (or equivalent) that constructs these mementos and packages them into a `.proof` bundle. The script's output, byte-for-byte, must hash to the pinned CID.

## The foundation key

A fixed Ed25519 keypair is used for self-contracts minting. The private key lives at `tools/foundation-key/foundation.priv` (or a similar canonical location); the public key is part of the protocol catalog. Every kit signs self-contracts with this same key, producing identical signatures because Ed25519 is deterministic given the key and the message.

The foundation key is not a secret. It is published in the protocol. Its purpose is to give every kit access to the same signing identity for the self-contracts package, so that the bytes match across kits.

A kit must NOT use the foundation key for any other purpose. Real contract mementos are signed with developer keys; the foundation key only signs the canonical self-contracts. This is enforced socially, not cryptographically; the conformance harness only checks that self-contracts bytes match.

## Why self-contracts is a strong check

Steps 2-4 have per-fixture tests: feed input X, get output Y. Self-contracts is different. It says: "construct bundle Z from scratch, using every layer of your kit, produce bundle Z's bytes."

Drift in any layer changes the bytes:

- Drift in the canonicalizer changes the inner CIDs of contract mementos.
- Drift in the claim envelope codec changes the outer CIDs of contract mementos.
- Drift in the CBOR encoder changes the bundle's outer CID.
- Drift in the signing path changes the bundle's signature, which changes the outer CID.
- Drift in the timestamp format, public key encoding, any small detail propagates to the outer CID.

The pinned outer CID is a single 64-byte test that exercises all of this. If your kit produces it, every layer is correct.

## How to mint

```
implementations/<your-language>/
├── mint-self-contracts/
│   ├── canonical-content/      # the inputs
│   │   ├── term-1.json
│   │   ├── term-2.json
│   │   ├── formula-1.json
│   │   └── ...
│   ├── mint.<your-language>    # the minting program
│   └── README.md
```

The minting program:

1. Reads the canonical content from `canonical-content/` (these inputs are byte-identical across kits; they are the protocol's, not the kit's).
2. Walks each input through the kit: canonicalize, hash, sign-as-claim-envelope, collect.
3. Constructs a proof bundle from the collected envelopes.
4. Signs the bundle with the foundation key.
5. Writes the result to stdout (or to `target/self-contracts.proof`).

The harness compares the output's CID against the pinned value. Pass = green. Fail = drift somewhere.

## Pinning

The pinned CIDs live in the protocol catalog and in `make conformance`. When a kit ships, its self-contracts CID is added to the pinned list. CI fails if any kit's mint output drifts.

When you add your kit to the harness, the pinned CID is initially empty; you mint, observe the CID, paste it into the pinned list. From that moment on, any drift in your kit causes CI failure.

This is the moment your kit becomes load-bearing. You are now claiming, by pinning, that your kit's bytes will agree with the protocol's bytes forever (or until the protocol version bumps).

## Protocol version bumps

When the protocol catalog version changes (v1.1.0 → v1.2.0), the canonical content of self-contracts may change. The pinned CIDs change. Every kit re-mints and re-pins.

This is one of the few coordinated activities in ProvekIt. A v1.2.0 release is gated on every kit re-minting and re-pinning successfully. If a kit fails to re-mint (because v1.2.0 added a new IR primitive the kit doesn't yet support, for example), the kit falls back to "v1.1.0 only" until it adds support.

## Common mistakes

- **Hardcoding the pinned CID in your kit's source.** Don't. The pinned CID lives in the harness configuration, not in kit source. Kit source produces bytes; harness compares.
- **Using a non-deterministic signing source.** Ed25519 is deterministic; if your bytes don't match, your *bytes-to-be-signed* don't match. Look upstream.
- **Skipping a member.** Every canonical content file becomes a member; if your kit's mint produces a bundle with fewer members than the canonical bundle, the outer CID won't match.
- **Including a member out of order.** Self-contracts members are in CBOR map-key order (CID-lex). Sort.

## Bridge IR gap

Several kits today (C#, Java, Ruby, C++) ship "partial" bridge IR: they pass the happy-path bytes-equality fixture but cannot construct or round-trip the full v1.1.0 9-field BridgeDeclaration. These kits' self-contracts may include a bridge member, which means partial-bridge kits cannot fully mint self-contracts.

This is a known issue tracked per-kit (see [`docs/reference/per-language-status.md`](../../reference/per-language-status.md)). Step 6 covers the full bridge IR.

If you are porting a new kit, do step 6 before pinning self-contracts. If you ship without full bridge support, your kit is "self-contracts-partial": usable but not fully conformant.

## When this step is done

Your kit's `mint-self-contracts` produces a bundle whose outer CID matches a pinned value. `make conformance` for your kit is fully green. Any future drift will fail CI.

## Read next

- [06-bridge-IR.md](06-bridge-IR.md): the full v1.1.0 BridgeDeclaration shape.
- [docs/contributing/release-process.md](../release-process.md) (when written): protocol version bumps and re-pinning.
