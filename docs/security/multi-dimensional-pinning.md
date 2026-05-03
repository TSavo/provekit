# Multi-dimensional pinning: the rank-N pin

This is the v1.4 architecture for closing supply-chain attack classes that single-CID pinning leaves open. It is the operational realization of the manifesto's §11 (the address as multi-dimensional) and §12 (the pin as a tuple).

If you read prior versions of `threat-model.md` or `what-binaryCid-catches.md` and concluded that "lying contracts" or "signed-but-mis-stated contracts" were the protocol's most important non-catch, that conclusion held under v1.1 framing. Under v1.4's multi-dimensional pinning, the non-catch shrinks materially. This doc walks the new picture.

## The four pinning dimensions

The same content has different addresses depending on which axes the projection includes:

| Dimension | What it projects over | Same content → same address? |
|---|---|---|
| `contractCid` | One ContractDecl (JCS bytes only) | YES — signer-independent |
| `contractSetCid` | A sorted set of contractCids | YES — order-independent |
| `attestationCid` | (contractCid + signer + declaredAt + signature) | NO — different witnesses produce different CIDs of the same content |
| Bundle file CID | content + envelopes + minting state + disk layout | NO — drifts on every honest re-mint |

The first two are **content-only** projections: any party with the content computes the same CID. The third and fourth are **signer-and-time-addressed**: by design distinguishable across witnesses.

The substrate's primary act is choosing which dimension to project through. **Choosing the wrong dimension is the failure mode.** Pin a bundle file's bytes as if they were a contract identity, and the address moves every time anyone re-mints — same contract, different address, because the dimension included signer state and mint timestamps.

The pre-v1.4 protocol conflated these axes. The v1.4 specs (`contract-cid-vs-attestation-cid`, `contract-set-extension`, `substrate-layers-envelope-header-body`, `version-chains-pinning`, `bridge-target-dimensionality`) are the substrate naming the dimensions it is willing to converge on.

## The rank: tuples, not single CIDs

Choosing dimensionality is half the discipline. Rank is the other half.

A single CID is rank-1. It expresses existence ("this content exists"). It does not express a relation ("this content satisfies that"). Relations live at rank 2 or higher.

| Assertion | Rank | The tuple |
|---|---|---|
| "This content exists" | 1 | `(cid)` |
| "This binary fulfills this contract" | 2 | `(binaryCid, contractCid)` |
| "This witness chain attests that this binary fulfills this contract" | 3 | `(witnessCid, contractCid, binaryCid)` |
| "This binary satisfies this contract under this build provenance" | 4 | `(binaryCid, contractCid, buildProvenanceCid, witnessCid)` |

A rank-N relation pinned as a single CID loses a predicate. The lost dimension does not vanish; it leaks back as drift. A 2-relation pinned as a single bundle-file CID looks stable when both axes happen to align and unstable whenever one moves while the other doesn't — the unstable case looks like a hash bug rather than a rank bug.

The discipline: name the relation you're claiming, count its axes, build a tuple of that rank, project each axis through a content-only dimension. The rest is the substrate doing what it already does.

## The canonical rank-3 consumer pin

The consumer's three-axis pin is `(contractCid, witnessCid, binaryCid)`:

- **`contractCid`**: what the contract claims (signer-independent; computed from contract bytes alone).
- **`witnessCid`**: the evidence (signed memento attesting `post → pre`; signer-specific; pinned by which witnesses the consumer trusts).
- **`binaryCid`**: the bytes that are running (computed from the compiled artifact at verification time).

Each axis is independently checkable. To attack, an adversary must coherently substitute all three:

1. Compile a malicious binary (changes `binaryCid`).
2. Forge a contract that claims the malicious binary does what the consumer expected (changes `contractCid`).
3. Produce a witness signed by a key the consumer trusts (requires compromising a trusted prover signing key).

Each is independently attackable. The combined attack requires three independent compromises. That bar is much higher than v1.1's single-CID attack surface.

## What this catches that v1.1's single-CID pinning didn't

### Lying contracts paired with matching binaries

**Pre-v1.4 framing:** an attacker writes a malicious function, signs a contract claiming the function does what consumers expect, ships the malicious binary alongside the lying contract. `binaryCid` matches; signature is valid; the verifier accepts.

**v1.4 framing:** the consumer's pin is `(contractCid, witnessCid, binaryCid)`. The attacker's substitution would have:

- A new contract whose `contractCid` differs from what the consumer pinned.
- OR, if the attacker forges the same `contractCid` somehow, a witness whose signing key is not in the consumer's trust set.
- OR, if the attacker has a trusted prover key, they can mint a witness — but that's a key-compromise attack, not a contract-substitution attack.

In all paths, the consumer's rank-3 pin breaks. The "lying contract paired with matching binary" attack class is closed at the rank-3 level. The remaining residue — "trusted prover key compromise produces fraudulent witnesses" — is exactly what is structurally required: a key compromise is needed to attack a key-rooted system. That's the irreducible TCB.

### Re-attestation by different signers

**Pre-v1.4:** Bob signs a contract; Alice maliciously re-signs the same content; in pre-v1.4 these had different "contract CIDs" because the term conflated content with signer.

**v1.4:** `contractCid` is content-only. Both Bob's attestation and Alice's attestation share a `contractCid`; their `attestationCid` values differ. The consumer pins `contractCid` and chooses whose attestations to trust separately. Re-attestation extends the witness chain; it does not break consumer pins.

### Version downgrade attacks

**Pre-v1.4:** an attacker ships an old vulnerable version of a library at a new version label. The consumer's version-string-based pin (`"^18.2.0"`) doesn't catch this because version strings are honor-system.

**v1.4:** `contract-set-extension` requires `previousContractSetCid` to chain back to the consumer's pinned base. A downgrade breaks the chain — `oldSet ⊆ newSet` fails. The consumer's `~18.2.0` resolution becomes a DAG walk, not a string match.

### Shim poisoning / placeholder strings

**Pre-v1.4:** bridges had `targetContractCid: "pending-csharp-counterpart:<name>"` or `targetProofCid: "deferred:phase-3-proof-bundle"`. These are anti-substrate: the field name says "CID" but the value is not a CID. They violate closure (manifesto §10) — the substrate cannot compute a CID for the named target.

**v1.4:** `bridge-target-dimensionality` mandates tagged-union targets. Either `{"kind": "contract", "cid": <real CID>}` or `{"kind": "contractSet", "cid": <real CID>}`. Placeholder strings are forbidden at the spec level; conformance tests reject them. **OMIT, don't stringify** — if a witness or binary axis is unknown at mint time, the corresponding body field is absent, not a string placeholder.

## What this does NOT catch

Even with rank-3 pinning, the residue:

- **Compromise of a trusted prover key.** If the attacker has a prover key the consumer trusts, they can mint witnesses for any contract / binary pair. Mitigation: hardware-key signing for prover keys; quorum signing; revocation lists.
- **Compromise of all axes simultaneously.** If the attacker controls the contract author's key + a trusted prover key + the binary's distribution channel, they can ship a coherent rank-3 lie. This is the multi-key compromise scenario; the bar is structurally higher than single-CID attacks but not infinite.
- **Side channels, timing, resource exhaustion.** Behavioral verification doesn't capture non-functional properties.
- **Compiler backdoors active during proof mint.** Discussed in [`what-binaryCid-does-not-catch.md`](what-binaryCid-does-not-catch.md). Trusting Trust still applies; reproducible builds are the complement.

The residue is narrower than under v1.1 framing. The "lying signers + matching binary" attack class moves from "structurally not caught" to "requires multi-key compromise." That's a categorical improvement.

## How a consumer mints a rank-3 attestation

The bridge declaration carries the contract-axis claim in its header and the witness/binary axes in its body (per `bridge-target-dimensionality`). The consumer reads all three CIDs and signs their own attestation:

```json
{
  "envelope": {
    "signer": "ed25519:<consumer-key>",
    "declaredAt": "2026-05-15T00:00:00Z",
    "signature": "ed25519:..."
  },
  "header": {
    "schemaVersion": "1",
    "kind": "consumer-attestation",
    "cid": "blake3-512:<this attestation's content hash>"
  },
  "metadata": {
    "contractCid": "blake3-512:<the contract this consumer is binding to>",
    "witnessCid":  "blake3-512:<the witness chain endorsing the contract>",
    "binaryCid":   "blake3-512:<the binary the consumer verified against>",
    "verifiedAt":  "2026-05-15T00:00:00Z"
  }
}
```

The consumer's signature transitively binds all three CIDs. To break the consumer's pin, an attacker would need to either:

- Compromise the consumer's signing key (defeats any pin).
- Find a tuple substitution `(contractCid', witnessCid', binaryCid')` that the consumer's verifier accepts under the same trust policy. Each component must independently pass its trust check.

The "find a tuple substitution" attack requires (at minimum) a forged binary, a forged contract that the binary actually satisfies, AND a forged witness signed by a key in the consumer's trust set. That last requirement is the irreducible bar.

## The substrate-vs-metadata cut

v1.4's envelope/header/body layering enforces what is and isn't substrate-load-bearing:

- **Envelope (signed)**: signer, declaredAt, signature. Substrate verifies the signature against the signer over JCS of `(header, body)`.
- **Header (substrate-verified)**: kind, cid, plus kind-specific required fields. Substrate verifies the four invariants over header content references.
- **Body (tooling-interpreted)**: everything else, signed under the envelope's signature. Substrate is opaque to body; tooling reads body fields for domain-specific protocols.

Why this matters for multi-dimensional pinning: the witness and binary axes live in body, not in header. The substrate doesn't validate these — it can't, because their semantics are domain-specific (what does "binaryCid matches the running binary" mean when the binary is a `.dylib` vs. a Wasm module vs. a smart contract on-chain?). The consumer's tooling validates these per the consumer's threat model.

The substrate stays small. The composition layer (rank-N pinning, version chains, witness DAGs) carries the world.

## Where the rank-3 pin meets the standardization argument

The v1.4 multi-dimensional pinning is the answer to the standardization-readiness question. Standards like DO-178C, Common Criteria EAL5+, ISO 26262 ASIL-D require formal verification at specific assurance levels. The reviewer-side question is always: "how do we know the running binary corresponds to the formally verified specification?"

Single-CID pinning answers: "trust the signature." That answer is acceptable to no high-assurance regime.

Rank-3 pinning answers: "the binary's hash is checked at runtime against `binaryCid`; the contract is identified by its content-only `contractCid`; the witness chain is signed by a prover whose backend the regime accepts; all three are bound together by the consumer's own signed attestation." Each axis has a distinct adversarial model and a distinct verification mechanism. This is the shape regulators want.

The standardization argument in [`../papers/04-vertical-stack-and-standardization.md`](../papers/04-vertical-stack-and-standardization.md) becomes substantially stronger under v1.4: the rank-3 pin is precisely the multi-axis verification regulators have always asked for, expressed as content-addressed CIDs with mathematically-defined composition.

## Read next

- [`../papers/03-substrate-not-blockchain.md`](../papers/03-substrate-not-blockchain.md) — the manifesto. §11 (address-as-multi-dimensional) and §12 (pin-as-tuple).
- [`threat-model.md`](threat-model.md) — the full threat coverage matrix, updated for v1.4.
- [`what-binaryCid-catches.md`](what-binaryCid-catches.md) — `binaryCid` as one axis of three.
- [`supply-chain.md`](supply-chain.md) — supply-chain attack scenarios under rank-3 pinning.
- [`../papers/04-vertical-stack-and-standardization.md`](../papers/04-vertical-stack-and-standardization.md) — why this matters for DO-178C / Common Criteria / ISO 26262.
- The v1.4 specs themselves: `protocol/specs/2026-05-03-bridge-target-dimensionality.md`, `protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md`, `protocol/specs/2026-05-03-contract-set-extension.md`, `protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md`, `protocol/specs/2026-05-03-version-chains-pinning.md`.
