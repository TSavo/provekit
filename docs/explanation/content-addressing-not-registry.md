# Content-addressing, not a registry

Sugar's "registry" is the BLAKE3-512 hashspace. There is no master copy. There is no service that mediates membership. There is no party whose downtime stops the protocol.

This doc unpacks why content-addressing is the right primitive, and what it changes about how the protocol can be operated, scaled, and trusted.

## What "content-addressing" means

A content-addressed system identifies data by its content rather than by its location. A traditional system uses location-addressing:

```
GET https://registry.example.com/packages/lodash/4.17.21
```

The URL is a location. The server decides what's at that URL. The client trusts the server's response.

A content-addressed system uses identity-from-content:

```
GET blake3-512:9d57c5e4...
```

The address is a hash of the content. Any party with the content can produce it; no one without the content can. The client can verify the response against the address: hash the bytes, compare to the address.

Bitcoin, Git, BitTorrent, IPFS all use this primitive. Sugar does too.

## What changes when you content-address

### No central authority

Bitcoin has no central mint; coin custody is decided by signatures over UTXOs. Git has no master copy; the same commit hash exists in every clone. BitTorrent has no central server; the seeders, leechers, and trackers are interchangeable. IPFS has no registry; the content addresses are the lookup keys.

Sugar has no central registry. Mementos are content-addressed; anyone with the bytes can verify; no party's permission is required to publish or consume.

This is structural. The protocol's correctness does not depend on any party being honest, online, or available.

### No invalidation

When bytes change, hashes change. Old mementos remain valid against the bytes they pinned; they simply become unreachable from re-canonicalized contracts. Stale data does not poison; orphaned mementos do not falsify.

This is the absence of cache invalidation. In a conventional cache:

- A cache entry can become stale (the underlying data changed; the cache hasn't refreshed).
- Stale entries can lead to wrong answers.
- Cache invalidation is a continuous coordination problem.

In Sugar:

- A memento is only valid for the bytes it pinned.
- If the bytes change, the new bytes hash to a different CID; the old memento is irrelevant to the new bytes; the new bytes need a new memento.
- No invalidation; just supersession.

This is what "provability is monotonic" means structurally. See [monotonic-provability.md](monotonic-provability.md).

### Federated discovery

A consumer looking for "the contract for `parseInt`" does not query a registry. They have one of:

- A specific CID they want to verify against (their build pinned it).
- A `.proof` they downloaded with their dependency (the bytes are local).
- A shared implication server they can pull from.

In none of these cases is there a central authority deciding what `parseInt` means. The bytes do.

If a consumer wants a public registry-like experience, they can run an indexer. The indexer is purely additive: it doesn't change the protocol's semantics; it just makes discovery convenient. Multiple competing indexers can exist; all are correct as long as they don't claim mementos that don't hash to their advertised CIDs.

### Permissionless publication

Sugar asks no party's permission to publish. The act of publishing is the act of producing bytes that verify themselves: a signed memento whose CID is its content. Anyone with a key pair can mint mementos. Anyone with the spec can verify them.

This is the lineage of Bitcoin, BitTorrent, Tor: protocols that operate without permission because they do not need one. The trust comes from the protocol's primitives, not from a gatekeeper.

A future ecosystem might have curators (reference-contract authors, implication-server operators), but no party is structurally privileged.

## What problems content-addressing solves

### The "what version is this really?" problem

Traditional dependency management has a chronic problem: the semantic version doesn't fully capture what's there. `lodash@4.17.21` could mean different things on different days if the registry has been compromised. The version is a name; the bytes are the truth, but the name doesn't pin the bytes.

Content-addressing fixes this. `blake3-512:9d57c5e4...` is the bytes. There is exactly one binary that hashes to that CID (modulo collision-finding in BLAKE3-512, which is computationally infeasible). The version-name lookup is a convenience; the CID is the truth.

### The "did this really come from where I think it came from?" problem

Traditional package distribution: the consumer trusts the registry to serve the right bytes. If the registry is compromised, the consumer is exposed.

Content-addressing fixes this. The consumer fetches by CID. If the bytes don't hash to the requested CID, the fetch is invalid. Tampering in transit is detected mathematically, not heuristically.

### The "which mirror has the right bytes?" problem

Traditional package distribution: each mirror has to be trusted independently. A compromised mirror can serve different bytes than the canonical one.

Content-addressing fixes this. All mirrors serve the same CID; the bytes either hash to the CID or they don't. The mirror's identity doesn't matter; the bytes do.

### The "how do I find this thing?" problem (partially)

Content-addressing alone doesn't solve discovery. You need to know the CID to fetch. Indexers and registries can map names to CIDs, but the indexer is a convenience, not a source of truth.

In Sugar's specific case: a consumer's `.proof` references dependencies' contract CIDs. The consumer fetches each by CID; resolution is by direct lookup, not by name. Publishers publicize CIDs (via package metadata, registries, or word of mouth); consumers fetch by CID.

## What problems content-addressing doesn't solve

### Discovery still requires indexers (or word of mouth)

You can fetch by CID if you know the CID. You don't always know the CID. Indexers are the practical answer; running an indexer is straightforward but not free.

Sugar does not prescribe how indexers should work. Multiple competing indexers can exist; all are correct as long as they verify the CIDs they advertise.

### Trust decisions are still local

Content-addressing tells you "these bytes" matched their CID and signature. It doesn't tell you whether to trust the signer. The trust decision (whose keys to trust, whose contract claims to accept) is local to each verifier.

This is a feature: no global trust authority. It's also work: each operator decides their own trust policy.

### Storage costs are still real

Mementos take bytes. A `.proof` for a large dependency tree can be megabytes. The protocol's storage requirements are not zero; they're amortized but not eliminated.

Mitigation: deduplication. The same canonical IR has the same CID; the same memento need only be stored once across the entire ecosystem. Indexers exploit this aggressively.

### Attacker can fork

Content-addressing is permissionless, including for adversaries. An attacker can mint signed mementos that claim wrong things; the protocol stores them; the protocol does not validate truthfulness.

The trust-set decision (whose keys are trusted) is the defense. Attackers' keys aren't trusted; their mementos are rejected.

## Why this lineage

Bitcoin showed that content-addressing scales for currency without a central mint. Git showed it scales for source history without a master copy. BitTorrent showed it scales for petabytes of content without a central server. IPFS showed it scales for the addressable web without a registry.

In each case, the conventional wisdom was that a central authority was structurally required. In each case, content-addressing showed it wasn't. The pattern is robust: domains thought to require central authorities admit content-addressed protocols.

Sugar is one more application of the same primitive. The domain is behavioral verification. The conventional wisdom would say "of course you need a central authority to certify proofs." Sugar says: no, the bytes are the certificate; the authority is the math.

## Operational consequences

What being content-addressed enables:

1. **Anyone can run an indexer.** No license, no permission, no API key.
2. **Mirroring is automatic.** Cache-friendly; CDN-friendly; immutable; safe.
3. **Tamper-evidence is structural.** Modified bytes mismatch the CID; rejection is mechanical.
4. **Distribution is decoupled from authentication.** TLS optional (CIDs verify integrity).
5. **Composition is hash-bounded.** The whole point of the protocol's cost model.
6. **Versioning is monotonic.** New versions are new CIDs; old CIDs remain pinned to old bytes.

What being content-addressed forbids:

1. **Mutating in place.** Cannot. The CID is the bytes; the bytes are immutable.
2. **Recalling a published memento.** Cannot. Once the bytes are out, anyone with them has them.
3. **Centralized takedown.** No central party to issue takedowns. Mitigation must be at the trust-set layer (revoke trusted keys; specific verifiers reject specific mementos).
4. **Backwards-incompatible changes that "just work."** New CIDs require explicit upgrade in consumer pinning.

## Read next

- [monotonic-provability.md](monotonic-provability.md): what monotonicity means and why it matters.
- [cross-domain-verification.md](cross-domain-verification.md): how content-addressing enables cross-language transfer.
- [thesis.md](thesis.md): the full claim.
- [`../security/signature-and-non-repudiation.md`](../security/signature-and-non-repudiation.md): signature scheme over the content-addressed bytes.
