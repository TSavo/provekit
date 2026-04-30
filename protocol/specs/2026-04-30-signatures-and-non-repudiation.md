# ProvekIt: Signatures and Non-Repudiation

> Author: shared session 2026-04-30 (T + Claude). The signature
> protocol that makes propertyHash composition load-bearing. Companion
> to `2026-04-29-the-semantic-envelope.md` (architectural context),
> `2026-04-29-supply-chain-via-semantic-envelope.md` (operational
> consequence), `2026-04-29-universal-claim-envelope.md` (wrapper
> schema this spec extends), and the canonicalization-grammar and
> memento-envelope sibling specs (canonical bytes this spec hashes).

## 1. Status and authority

This document is a **protocol specification**. The protocol is
authoritative. Any TypeScript, Rust, or other implementation is a
reference, not the source of truth. Where the current code at
`src/claimEnvelope/sign.ts`, `src/producerKeys/index.ts`, or
`src/fix/runtime/mementoStore.ts` differs from this spec, the
implementation must be aligned to the spec. Implementation drift
detected at the time of writing is enumerated in §13.

Conformance language follows RFC 2119 and RFC 8174. **MUST**, **MUST
NOT**, **SHOULD**, **SHOULD NOT**, **MAY** are interpreted as defined
there.

## 2. Why signatures matter

ProvekIt mementos are content-addressed. A CID answers the question
"are these the same bytes?" It does not answer the question "who
attested to these bytes?" That second question is the load-bearing
foundation of the supply-chain argument: a library author signing
propertyHash X stakes their key, their reputation, and their
institutional credibility on the claim that X holds for the library's
code. Without signatures, anyone can mint a memento; with signatures,
mementos carry transferable accountability.

The supply-chain spec
(`2026-04-29-supply-chain-via-semantic-envelope.md`) shows how
propertyHashes compose across libraries to detect semantic-level
divergence in dependency bumps. That argument depends, implicitly, on
the proposition that the signatures attached to those propertyHashes
are non-repudiable: if a library author later denies signing
propertyHash X, the signature is the receipt that contradicts the
denial. This document formalizes the mechanics.

## 3. Scope and non-scope

**In scope.** Key formats; what gets signed; signature serialization;
identity binding; revocation; non-repudiation properties;
multi-signature; fail-closed verification; conformance criteria.

**Out of scope.** Trust roots and trust anchors (§11); canonicalization
(see canonicalization-grammar sibling spec); memento envelope shapes
(see `2026-04-29-universal-claim-envelope.md` and the memento-envelope
sibling spec); chain-of-custody validity rules; provenance-DAG
walking; the existential supply-chain layers 1-4 already covered by
SLSA, sigstore, in-toto, TUF, etc.

## 4. Key formats

### 4.1 MUST-support algorithm

A conforming implementation **MUST** support Ed25519 (RFC 8032) as a
keypair algorithm. Ed25519 is selected because:

- It is the modern default for new protocols (TLS 1.3 cipher suites,
  SSH, age, signify, sigstore Cosign).
- Public keys are 32 bytes; signatures are 64 bytes; deterministic
  signing eliminates an entire class of nonce-reuse attacks present
  in ECDSA.
- Node, Go, Rust, Python, and the WebCrypto API all expose first-class
  Ed25519 support, so cross-language conformance is mechanically
  achievable.

### 4.2 Algorithm extensibility

The protocol **MUST** carry an explicit algorithm discriminator in
every signed structure (the `alg` field in §6.2). Conforming
implementations **MUST** reject any structure whose `alg` they do not
recognize (fail-closed; see §10).

The algorithm discriminator is a string drawn from the following
registry. New algorithms are added to the registry by the
specification-editing process; implementations that support a new
algorithm **MUST** still reject signatures with unknown `alg` values
they themselves do not implement.

```
alg-id        ::= "ed25519"               ; MUST-support, this version
                | "ed448"                 ; reserved
                | "secp256r1-ecdsa-sha256" ; reserved
                | "secp256k1-ecdsa-sha256" ; reserved
                | "ml-dsa-65"             ; reserved (post-quantum, NIST FIPS 204)
                | "ml-dsa-87"             ; reserved (post-quantum, NIST FIPS 204)
```

Reserved discriminators in this version of the spec **MUST NOT** be
used by producers; reservation prevents future namespace collision.

### 4.3 Key encoding on the wire

Public keys appearing inside memento bodies (key-publication mementos,
multi-sig signer references) **MUST** be encoded as multibase strings
(RFC draft-multiformats-multibase) using the `base64url` prefix `u`,
followed by the raw key bytes. For Ed25519 this is exactly 33 bytes
of multibase-encoded text representing 32 bytes of public key
material.

Implementations **MAY** accept additional encodings (DER pkcs8 or
SubjectPublicKeyInfo) at their input boundaries for ergonomic reasons,
but the canonical on-the-wire form **MUST** be multibase
`base64url`-prefixed raw bytes. Two implementations that disagree on
encoding produce different CIDs.

Private keys are out of scope of the wire format; they never appear in
mementos.

## 5. What gets signed

### 5.1 Per-memento type signing requirements

The universal claim envelope schema
(`2026-04-29-universal-claim-envelope.md`) specifies that
`producerSignature` is *optional*. This protocol refines that:
signatures are **required** or **forbidden** based on memento role and
distribution boundary. The matrix below is normative.

| Memento role                                        | Signature requirement | Rationale                                                                |
|-----------------------------------------------------|-----------------------|--------------------------------------------------------------------------|
| Catalog memento (published library invariants)      | **MUST** be signed    | Crosses publication boundary; consumers' bridges depend on it.           |
| Bridge memento (consumer-side dependency on a propertyHash) | **MUST** be signed    | Crosses build boundary; downstream proofHashes compose it.               |
| ProofHash root memento (a binary's terminal claim)  | **MUST** be signed    | Stakes the producer's claim that the named proofHash holds.              |
| Producer-public-key memento (key publication)       | **MUST** be self-signed | Key publication is a non-repudiable assertion of identity.               |
| Key-rotation memento                                | **MUST** be signed by the *old* key | The old key authorizes the rotation; see §7.2.                           |
| Key-revocation memento                              | **MUST** be signed by the *revoking* identity | See §7.3 for the identity rules.                                         |
| Verdict memento (z3-model, z3-unsat, type-check-pass, lint-pass, test-pass, test-fail, mutation-witness, workflow-run, llm-proposal, pattern-match) | **MUST** be signed when distributed; **MAY** be unsigned when local | A swarm-distributed verdict is a transferable claim; a local cache row is not. |
| Audit-only telemetry memento                        | **MUST NOT** be signed | Explicit non-attestation; presence of a signature would mislead consumers.|
| Legacy-witness memento (pre-spec backward compat)   | **SHOULD** be signed when distributed; **MUST** be tagged unsigned otherwise | Carries no semantic claim of its own.                                    |

A consumer that receives a memento across a process or distribution
boundary (see §5.2) **MUST** reject it if its role requires a
signature and the signature is missing, malformed, or invalid.

### 5.2 Distribution boundary

A *distribution boundary* is any of:

- A memento crossing a process boundary (IPC, network, filesystem
  shared between users).
- A memento referenced by CID from a published catalog, package
  manifest, lockfile, or other artifact intended to be consumed by a
  party other than the producer.
- A memento read from a swarm peer.

Within a single process and a single trust principal (the local
ProvekIt cache, e.g.), unsigned mementos are operationally tolerable.
The boundary determines the requirement.

### 5.3 Canonical bytes per memento type

What gets signed is the *canonical byte representation* of the
memento envelope with the following fields elided during canonical
encoding:

- The `cid` field (self-reference).
- The `signatures` field (self-reference; replaces the
  `producerSignature` field of the prior single-sig design; see §6).

All other fields **MUST** be included. Canonical encoding is defined
in the canonicalization-grammar sibling spec; the bytes hashed for
signature are byte-identical to the bytes hashed for CID computation.

This means: **the signature covers exactly the canonical bytes that
also constitute the memento's content identity.** A signature
verifies the producer attested to the bytes whose CID identifies the
memento, modulo the elided self-referential fields.

For memento types whose body shapes are defined in the
memento-envelope sibling spec, the canonical bytes are the bytes that
spec specifies. This document does not redefine memento body shapes.

## 6. Signature serialization

### 6.1 Field placement

The single-signature `producerSignature` field defined in
`2026-04-29-universal-claim-envelope.md` is **superseded** by a
`signatures` field that holds an *unordered set* of signature
structures. A memento with exactly one signer has a `signatures`
array of length one; multi-signed mementos have length > 1 (§9).

```
ClaimEnvelope.signatures ::= [ Signature ... ]   ; possibly empty for unsigned
```

### 6.2 Signature structure

Each element of `signatures` is a JSON object with the following
fields:

```
Signature ::= {
  "alg":   alg-id,            ; §4.2 registry; MUST match the signer's key alg
  "signer": cid32,            ; CID of the producer-public-key memento (§7.1)
  "sig":   multibase-bytes,   ; multibase-encoded raw signature bytes
  "ts":    iso8601-utc        ; signer's wall-clock timestamp at signing
}
```

- `alg` is the algorithm discriminator from §4.2.
- `signer` is the 32-hex-char CID (sha256-prefix-32) of the
  producer-public-key memento that publishes the public key against
  which the signature verifies. See §7.1.
- `sig` is multibase-encoded raw signature bytes (`base64url` prefix
  `u`). For Ed25519 this is `u` followed by 86 base64url characters
  encoding 64 raw signature bytes.
- `ts` is the signer's claimed signing wall-clock time, ISO-8601 UTC.
  Used for ordering against revocation timestamps (§7.3, §10).

The `signatures` array **MUST** be sorted in canonical order
(lexicographic by `signer` CID, ties broken by `ts`, ties broken by
`sig`) so that two consumers re-canonicalize identical bytes.

### 6.3 Body-then-signature ordering

The protocol pins the order: the body is canonicalized first; the
signatures are computed over the canonical body bytes (with
`signatures` and `cid` elided); each signature is then attached to
the same envelope; the envelope is re-canonicalized (with
`signatures` now populated, `cid` still elided) for CID computation.

Concretely:

```
canonicalBody  = canonicalize(envelope minus {cid, signatures})
for each signer:
    sig_i      = signer_i.sign(canonicalBody)
envelope.signatures = sort([{alg, signer, sig: sig_i, ts}, ...])
canonicalEnvelope    = canonicalize(envelope minus {cid})
envelope.cid         = sha256(canonicalEnvelope)[:32 hex chars]
```

This ordering is normative. Implementations **MUST NOT** sign over
the post-CID envelope.

## 7. Identity binding

### 7.1 Signer is a CID

Each signature's `signer` field is the CID of a *producer-public-key
memento*. That memento is a regular memento envelope whose body
contains the multibase-encoded public key, the algorithm
discriminator, the producer-id string (`<name>@<version>`), and the
publication timestamp. A producer-public-key memento **MUST** be
self-signed by the private key it publishes (proof of possession).

```
ProducerPublicKeyMemento.evidence ::= {
  "kind":   "producer-public-key",
  "schema": cid32,
  "body":   {
    "alg":         alg-id,
    "publicKey":   multibase-bytes,
    "producerId":  "<name>@<version>",
    "publishedAt": iso8601-utc
  }
}
```

The CID of this memento is what every other signature's `signer`
field references. Resolving the public key is one CID lookup followed
by one body parse.

### 7.2 Why CID-of-key-memento and not raw key fingerprint

A raw key fingerprint identifies bytes but carries no metadata. The
CID of a key-publication memento additionally binds:

- The algorithm in use.
- The producer-id (so the same physical key cannot be reused under a
  different producer-id without a fresh memento and a fresh CID).
- The publication timestamp (used in revocation backdating defenses).
- The signer's self-attestation (proof of possession via the
  self-signature).

Resolving a `signer` field is therefore not just a key lookup; it is
a memento walk, which means the consumer's verifier already exercises
the same machinery used for every other content-addressed claim.

### 7.3 Rotation chains

A producer rotating from key K-old to key K-new **MUST** publish two
mementos:

1. The new producer-public-key memento (self-signed by K-new).
2. A `producer-key-rotation` memento, signed by K-old, whose body
   names both CIDs.

```
ProducerKeyRotationMemento.evidence ::= {
  "kind":   "producer-key-rotation",
  "schema": cid32,
  "body":   {
    "producerId": "<name>@<version>",
    "oldKeyCid":  cid32,
    "newKeyCid":  cid32,
    "rotatedAt":  iso8601-utc,
    "reason":     string  ; free-form
  }
}
```

The rotation memento itself is signed by K-old, which is what
authorizes the rotation. K-new's self-signature on its
producer-public-key memento is what attests possession.

A consumer resolving "the current key for producer P" walks the
rotation chain forward from the original key publication: K0 -> K1
(if a rotation memento for K0 exists) -> K2 (if a rotation memento
for K1 exists) -> ..., stopping at the first key with no successor
rotation memento. That terminal key is "current."

## 8. Revocation

### 8.1 Revocation memento

A `producer-key-revoke` memento marks a key as revoked from its
`revokedAt` timestamp onward.

```
ProducerKeyRevokeMemento.evidence ::= {
  "kind":   "producer-key-revoke",
  "schema": cid32,
  "body":   {
    "producerId":      "<name>@<version>",
    "revokedKeyCid":   cid32,
    "revokedAt":       iso8601-utc,
    "reason":          string,
    "successorKeyCid": cid32 | null  ; null if revocation is terminal
  }
}
```

### 8.2 Who can revoke

A revocation memento for key K **MUST** be signed by at least one of:

- K itself (self-revocation; the only path before key compromise is
  detected).
- The successor key K' (if K -> K' rotation is already established).
- A peer key holding a co-signing relationship pre-declared in K's
  publication memento (out of scope for v1; reserved field
  `cosigners` in the publication memento body).

A revocation signed only by a key not authorized by one of these
paths **MUST** be rejected.

### 8.3 Backdating limitation

Without an external transparency log or independent witness, the
`revokedAt` timestamp is producer-asserted. A compromised producer
controlling K could publish a revocation memento with a `revokedAt`
predating the compromise and use it to invalidate previously-signed
mementos retroactively. This is an honest limitation of this version
of the spec.

The protocol **provides** the following mitigations short of a
transparency log:

1. **Earliest-timestamp wins.** When multiple revocation mementos
   exist for the same key, consumers **MUST** treat the *earliest*
   `revokedAt` as the effective revocation time. A producer cannot
   "un-revoke" by publishing a later memento with a later timestamp.
2. **Memento `producedAt` ordering.** Each signed memento carries its
   own `producedAt`. A consumer rejects a memento whose `producedAt`
   is at-or-after the revocation's `revokedAt` for the signing key.
3. **Optional co-signed revocation.** Implementations **SHOULD**
   support revocation mementos co-signed by an independent peer key;
   the co-signer's timestamp is treated as the authoritative
   `revokedAt`.

A future revision of this spec will define a transparency-log
primitive (a Merkle log of revocation entries with periodic external
witnessing) that closes this gap. Until then, consumers requiring
stronger backdating guarantees layer their own attestation.

### 8.4 Effect on signature verification

Given a memento M with signature S whose `signer` resolves to key K:

- If K is not revoked, S is verifiable against K's public key. (The
  verification math is the operative check.)
- If K is revoked at time T_rev, and M's `producedAt` is strictly
  before T_rev, S **MAY** still be considered valid by consumer
  policy (the signature was made when the key was good).
- If K is revoked at time T_rev, and M's `producedAt` is at or after
  T_rev, S **MUST** be rejected.
- If K is revoked at time T_rev, and S's own `ts` field is at or
  after T_rev, S **MUST** be rejected (the signing event itself is
  post-revocation).

The default consumer policy **MUST** be the *strictest interpretation
consistent with the data*: when in doubt, reject. Implementations
**MAY** offer relaxed policies (accept signatures pre-dating
revocation when revocation reason is non-compromise key rotation) but
this is opt-in, not default.

## 9. Multi-signature

### 9.1 Semantics

The `signatures` array carries an *unordered set* of independent
signatures over the same canonical body bytes. Each signature is
verifiable independently.

The protocol itself does not encode AND, OR, or M-of-N policy.
Verification policy is consumer-specified. The default policy
**MUST** be:

- **All signatures MUST verify.** A memento with N signatures, of
  which any one fails verification (bad bytes, revoked key, unknown
  algorithm), **MUST** be rejected entirely. This is the "AND" /
  "implicit conjunction" reading.

Consumers **MAY** declare alternative policies (M-of-N, weighted by
signer role, OR-semantics for multi-rooted trust), but those policies
are layered above this protocol; the *protocol-level* default is "all
signatures present **MUST** all verify."

### 9.2 No sequential countersignature

This spec deliberately does not adopt sequential countersignature
(where signature N covers body + signatures 1..N-1). Sequential
ordering would impose ordering semantics that consumers may or may
not want and would prevent two parties from independently signing in
parallel and merging into one envelope. The canonical-set design
permits parallel signing and lossless merge.

If an application needs ordering (e.g., "auditor signs after author"),
the application **MUST** encode that semantics in evidence body fields
(timestamps, role declarations) or in a separate workflow memento that
references the multi-signed memento by CID. The signature layer
itself remains order-free.

### 9.3 Adding a signature later

Adding a signature to an existing memento changes the memento's CID
(because `signatures` is part of canonicalization). The
co-signed-later memento is therefore a *distinct memento*: it has a
new CID, may have a new `producedAt`, but covers the same canonical
body bytes. Consumers and tooling **MUST NOT** assume CID stability
across signature additions. The shared identity is the canonical body
bytes; the CID is the bytes-plus-signatures identity.

## 10. Fail-closed verification

A conforming verifier presented with a memento whose role requires a
signature **MUST** reject the memento under any of the following
conditions. "Reject" means: do not return a verdict to the caller, do
not treat the memento as having satisfied any property, do not
contribute the memento's verdict to cross-validation, and surface a
diagnostic naming the failure.

### 10.1 Required-reject conditions

A verifier **MUST** reject when:

1. The `signatures` array is empty or absent and the memento role
   requires signing (§5.1).
2. Any signature's `alg` is not in the implementation's supported
   algorithm set.
3. Any signature's `signer` CID does not resolve to a
   producer-public-key memento.
4. Any signature's `signer` resolves to a key whose `alg` does not
   match the signature's `alg`.
5. Any signature's `sig` does not verify against the resolved public
   key over the canonical body bytes (with `cid` and `signatures`
   elided).
6. Any signature's `ts` is at or after a revocation `revokedAt` for
   that signer key.
7. The memento's `producedAt` is at or after a revocation `revokedAt`
   for any signing key (§8.4).
8. The producer-public-key memento is itself unsigned or not
   self-signed.
9. The rotation chain for any signing key contains a cycle.
10. Any rotation memento in the chain is not signed by the
    immediately-preceding key.
11. Any field of `signatures` is malformed (non-string `alg`, invalid
    multibase encoding, non-CID `signer`, non-ISO8601 `ts`).
12. The signed canonical bytes computed by the verifier differ from
    those signed by the producer (i.e., re-canonicalization disagrees).

### 10.2 Default-reject for ambiguity

In any case the protocol does not explicitly specify, the verifier's
default action **MUST** be **reject**. Specifically:

- An unknown evidence-variant `kind` whose memento role requires
  signing is *not* exempted from the signature requirement.
- An unknown algorithm in the registry that the implementation
  recognizes-but-does-not-support is **MUST** reject (the
  implementation cannot verify it).
- A signature on a memento whose role forbids signing
  (audit-only telemetry, §5.1) **MUST** cause rejection on the basis
  of role mismatch.

Absence of evidence is evidence of absence. The protocol is
fail-closed by construction.

### 10.3 No "warn" tier

Earlier drafts (and the current implementation, see §13) permit a
"warn" tier for locally-produced unsigned mementos. This protocol
**deletes** that tier. A memento either passes verification or it
does not. Local trust decisions (e.g., "I don't enforce signatures on
my own development cache") are *configuration policy at the consumer*,
not a protocol-level relaxation. The protocol's default behavior is
strict.

## 11. Trust roots are out of scope

ProvekIt **MUST NOT** ship with a hardcoded trust anchor, root CA, or
default key registry. The framework provides:

- The signature verification primitive (§10).
- The revocation lookup primitive (§8).
- The rotation walk primitive (§7.3).

The framework does **not** decide who any particular consumer should
trust. Trust-root selection is a per-consumer policy decision and
**MUST** live above the protocol.

Consequence for implementations: there is no `provekit verify`
default-allow list. Any binary that ships with hardcoded "ProvekIt
trusts these keys" is implementing a layer above the protocol and
**MUST NOT** present that policy as if it were part of the protocol.
Consumers may build their own trust-anchor mechanism (a project-level
config naming acceptable signer CIDs; a CI-level whitelist of
upstream library author keys; an organizational PKI) and layer it on
top.

This is the same separation that the semantic-envelope spec imposes
overall: ProvekIt has no PKI, no governance, no central trust root.
The math is neutral. Trust is the consumer's call.

## 12. Non-repudiation properties

Given a conforming implementation, the protocol provides the
following formal properties. Each is stated in MUST-form for what the
protocol guarantees; informal explanation follows.

### 12.1 Property A: authorship-of-bytes

Once producer P has signed memento M with key K, and the signature
has been resolved through a content-addressed publication path, P
**MUST NOT** be able to credibly later claim "I did not sign these
bytes."

Mechanically: the signature verifies against K's public key over the
canonical bytes of M; K's CID resolves to a publication memento whose
body declares K's owner; that publication memento is itself
self-signed by K. Repudiation therefore requires P to either
demonstrate K was compromised at signing time (a public claim with
its own non-repudiation cost) or demonstrate the canonical encoder is
broken (a verifiable mathematical claim).

### 12.2 Property B: bytes-of-claim

P **MUST NOT** be able to credibly claim "I signed something
different from what was published." The signature covers the
canonical bytes; canonical encoding is deterministic; two consumers
re-canonicalize identical inputs to identical bytes. There is no
ambiguity in "what bytes the signature covers."

### 12.3 Property C: revocation-audit-trail

P **MUST NOT** be able to retroactively pretend to have revoked at an
earlier date once a revocation memento has been published and
content-addressed. The memento's CID is fixed at publication; its
`revokedAt` is part of its canonical bytes; later revocation mementos
with later timestamps cannot displace earlier ones (§8.3 earliest-
timestamp wins).

P **CAN** still backdate the *first* revocation memento it ever
publishes for a given key, in the absence of an external witness.
This is the limitation §8.3 names openly.

### 12.4 Property D: cross-domain transferability

A signature from producer P **MUST** be verifiable by any consumer
implementing this protocol, regardless of the consumer's host
language, ecosystem, organizational affiliation, or trust-root
choice. Verification math is the same; canonical bytes are the same;
the `signer` CID resolves identically. This is what makes "the
proofHash composes across language and library boundaries" load-
bearing.

### 12.5 What the protocol does not provide

Honest enumeration of what is *not* covered:

- **Plausible deniability of key possession.** A signed memento
  proves the holder of K signed; if K's owner says "K was stolen
  from me," that is a separate evidentiary question this protocol
  does not adjudicate.
- **Key-compromise notification timeliness.** Without an external
  transparency log, the time between a compromise and the appearance
  of a revocation memento is unbounded.
- **Anonymity of signers.** Producer-id is bound to the publication
  memento; pseudonymity is achievable, but the same pseudonym across
  mementos is observable.
- **Forward secrecy of past signatures.** Ed25519 signatures remain
  verifiable indefinitely; revocation does not cryptographically
  invalidate prior signatures, only changes consumer policy.

## 13. Implementation drift

The current TypeScript reference implementation drifts from this spec
in the following specific ways. Each is flagged for follow-up
alignment; this spec is authoritative.

### 13.1 `producerSignature` (single signature) vs `signatures` (array)

`src/claimEnvelope/types.ts:239` defines `producerSignature?: string`
as a single optional base64-encoded signature. This spec replaces it
with `signatures: Signature[]`. Migration: all single-signed envelopes
become arrays of length one; the field name changes; multi-signature
becomes possible without further schema change.

Affected: `src/claimEnvelope/types.ts`, `src/claimEnvelope/sign.ts`,
`src/claimEnvelope/cid.ts` (`envelopeForHashing` must elide
`signatures` instead of `producerSignature`), every test that
constructs an envelope with `producerSignature`.

### 13.2 Signature fields incomplete

The current implementation stores only base64 signature bytes, with
no `alg`, `signer`, or `ts` fields. Identity binding currently
relies on out-of-band knowledge of which producer's key to verify
against. This spec requires the full `Signature` structure of §6.2.

Affected: `src/claimEnvelope/sign.ts:36` (`return sig.toString("base64")`
must return a `Signature` object); every caller of `signEnvelope`.

### 13.3 Encoding is base64, not multibase

The current implementation uses bare base64. This spec requires
multibase `base64url` (`u`-prefixed). Migration: producers prefix
existing values with `u` and switch from `base64` to `base64url`
alphabet.

Affected: `src/claimEnvelope/sign.ts:36`, every published key memento
body.

### 13.4 Validation is "warn for locally-produced"

`2026-04-29-universal-claim-envelope.md:312` permits a "warn for
locally-produced" tier. This spec deletes that tier (§10.3).

Affected: any verifier that consults `producerSignature` and chooses
between reject and warn based on origin.

### 13.5 Revocation backdating is uncontested

`src/producerKeys/index.ts` at the time of writing implements
revocation but does not enforce the earliest-timestamp-wins rule
(§8.3.1) and does not cross-check `producedAt` against `revokedAt`
on signature verification (§8.4). Consumers therefore accept
post-revocation signatures.

Affected: `src/producerKeys/index.ts` `verifyMemento`; the verifier
needs to read all revocation mementos for a signing key, take the
earliest `revokedAt`, and reject signatures whose `producedAt` is
at or after that timestamp.

### 13.6 Key mementos use `Memento` shape, not `ClaimEnvelope`

`src/producerKeys/index.ts` documents (lines 25-27) that key mementos
use the simpler `Memento` shape "as a bootstrap layer." This spec
**requires** key mementos to be full `ClaimEnvelope` envelopes with
the `producer-public-key`, `producer-key-rotation`, and
`producer-key-revoke` evidence variants defined in §7-§8. The
bootstrap-vs-envelope split is a pre-protocol artifact; once this
spec lands there is no bootstrap layer below the envelope.

Affected: `src/producerKeys/index.ts` end-to-end; `writeMemento`
producers for key mementos; any consumer that special-cases the
`Memento`-shaped key rows.

### 13.7 Rotation memento not signed by old key

The existing rotation memento (key `KIND_ROTATION` in
`src/producerKeys/index.ts:89`) is unsigned. This spec (§7.3)
requires rotation mementos to be signed by the *old* key, which is
what authorizes the rotation in the first place.

Affected: `src/producerKeys/index.ts` rotation flow; any test
asserting unsigned rotation works.

## 14. Conformance criteria

A claim of conformance to this spec **MUST** demonstrate the
following observable behaviors. Each criterion has a corresponding
test obligation.

### 14.1 Algorithm conformance

C1. Generate an Ed25519 keypair, publish a producer-public-key
memento (self-signed), sign a verdict memento with the keypair, and
verify the signature successfully against the published key.

C2. Reject a memento whose `signatures[i].alg` is `"ed448"`,
`"unknown-alg"`, or any value not in the implementation's supported
set.

C3. Reject a memento whose `signatures[i].alg` is `"ed25519"` but
whose `signer` resolves to a public key memento with `alg = "ed448"`.

### 14.2 Canonical-bytes conformance

C4. Two implementations in different host languages, given identical
envelope inputs, produce byte-identical canonical bodies and
byte-identical signatures over those bodies (modulo signature
non-determinism on non-deterministic algorithms; Ed25519 is
deterministic, so this is a strict byte-equality test for Ed25519).

C5. Modifying any field other than `cid` or `signatures` in the
envelope, then re-canonicalizing, produces a different signed-bytes
buffer (signatures over the original do not verify on the modified).

C6. Modifying `signatures[i].sig` by a single bit causes verification
to fail.

### 14.3 Revocation conformance

C7. Publish key K, sign a memento M at T1, publish revocation of K at
T2 > T1, verify M still passes (default policy permits pre-revocation
signatures unless reason indicates compromise).

C8. Publish key K, publish revocation at T2, sign a memento M at T3 >
T2, verify M is rejected with reason `revoked`.

C9. Publish two revocation mementos for the same key at T2 and T3 >
T2; verify the earliest (T2) is treated as the effective revocation
time.

C10. Publish a revocation memento signed by neither K nor a successor
key; verify it is rejected.

### 14.4 Multi-signature conformance

C11. Sign a memento with signers A and B, verify both signatures
verify, verify the memento passes overall.

C12. Sign a memento with signers A and B, modify B's `sig` by a bit;
verify the memento fails verification overall (default AND policy).

C13. Sign a memento with signer A, then independently sign with
signer B, merge into a single envelope; verify the merged envelope
passes.

C14. Adding signature B to a memento previously signed only by A
produces a memento with a different CID; the bytes-of-body identity
(everything but `cid` and `signatures`) is unchanged.

### 14.5 Fail-closed conformance

C15. For each of the conditions in §10.1, construct a malformed
memento and verify it is rejected with a diagnostic naming the
condition.

C16. Construct a memento whose role requires signing (catalog
memento, bridge memento, proofHash root) with empty `signatures`;
verify it is rejected.

C17. Construct an audit-only-role memento with a non-empty
`signatures` array; verify it is rejected (role-mismatch).

### 14.6 Trust-root neutrality conformance

C18. The conformance test suite **MUST NOT** depend on any specific
key being trusted. Tests publish their own keys, sign with their own
keys, and verify against their own keys. No "ProvekIt ships with key
X" assumption is permitted.

C19. The reference implementation **MUST NOT** ship a default
trust-anchor list; if it does, conformance fails.

A claim of conformance is structured as: an implementation, a list of
which of C1-C19 it passes, and a description of any deviations. C1-C18
are mandatory for protocol conformance; C19 is mandatory for
reference-implementation conformance specifically.

## 15. Acknowledgments

This spec extends the producer-signature scheme sketched at
`2026-04-29-universal-claim-envelope.md:334-358`. The single-signature
design and bootstrap-Memento-shaped key mementos in the existing
implementation are pre-protocol artifacts; they motivated this
formalization by surfacing what was actually load-bearing once
catalog composition (`2026-04-29-the-semantic-envelope.md`) and
supply-chain firewalling
(`2026-04-29-supply-chain-via-semantic-envelope.md`) were fully
specified.
