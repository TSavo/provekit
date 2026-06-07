# Monotonic provability

Hashes are deterministic functions of canonical bytes. When bytes change, hashes change. Old implication mementos remain cryptographically valid against their stated `(antecedentHash, consequentHash)`; they simply become unreachable from any contract that has been re-canonicalized. The lattice does not need invalidation.

This is the structural absence of cache invalidation. A stale entry in a conventional cache is a poison pill; in Sugar, an old memento describing now-orphaned hashes neither falsifies nor poisons anything. The lattice grows monotonically. Every minted implication memento is true forever, against the bytes it was minted for.

The implication: provability is monotonic. A fact, once published, is a hash lookup forever. The protocol's value compounds with time. Software ages backwards.

## What monotonicity actually means

In the conventional cache pattern:

- A cache stores `(key, value)` pairs.
- Underlying state changes; the value at `key` is now wrong; the cache is "stale."
- Stale entries can lead to wrong answers.
- Cache invalidation is required to remove stale entries.
- Cache invalidation is famously hard ("the two hardest problems in computer science").

In Sugar's lattice:

- An implication memento stores `(antecedentHash → consequentHash, evidence, signature)`.
- Underlying contracts can change. When they do, their canonical IR changes; their CIDs change.
- The old implication memento is still cryptographically valid against the *old* CIDs.
- It is just not useful anymore: nothing references the old CIDs.
- It is not invalidated; it is unreachable.

This is monotonicity. The lattice only grows. Old entries are never invalidated, only orphaned.

## Why this matters

### Operational simplicity

A conventional cache requires:

- Tracking dependencies.
- Computing what to invalidate when something changes.
- Coordinating invalidation across distributed cache layers.
- Handling failed invalidations.

Sugar requires none of this. You add to the lattice; you never remove. The lattice maintainer's job is "add new entries"; the maintainer's job is never "figure out what's stale."

### Distribution simplicity

A conventional cache shared across multiple parties:

- Each party has its own cache.
- Invalidation must propagate.
- A party that misses an invalidation has stale data.

Sugar's lattice shared across multiple parties:

- Each party has its own copy.
- New entries propagate; nothing needs to be removed.
- A party with an outdated copy has fewer entries; they don't get wrong answers, they just discharge fewer call sites at Tier 2.

This is a much weaker consistency requirement. Monotonic + eventually consistent + always-correct is achievable; the conventional alternative (strongly consistent invalidation) is much harder.

### Trust decay handled gracefully

If a developer's signing key is compromised in 2027, what happens to the implication mementos signed with that key in 2026?

In a conventional system: revoke the key, somehow propagate to all caches, hope the propagation reaches everyone.

In Sugar:

- The 2026 mementos remain cryptographically valid (the signature was correct against the bytes).
- Verifiers updated their `trusted_keys` configuration to reject the compromised key going forward.
- Each verifier's local trust decision is independent.
- Old mementos signed by the compromised key are rejected by verifiers with updated trust; old mementos are accepted by verifiers without updated trust.
- No global state to mutate.

The graceful degradation: each verifier's view of the lattice is its own. Trust changes are local. The lattice itself is unchanged.

### Disaster recovery

If an indexer goes down, the bytes are still there in every party's local copy. New verifiers cannot fetch from the dead indexer; they fetch from any other party that has the bytes. The lattice survives any single indexer's failure trivially.

This is a feature of content-addressing combined with monotonicity: no party is structurally privileged; all copies are interchangeable; loss of any single copy is recoverable from any other.

## What monotonicity does NOT provide

### Old mementos are NOT automatically applicable

Just because a memento is cryptographically valid forever doesn't mean it's useful forever. If a developer changes their function's behavior:

- Their function's annotation changes.
- The annotation's lifted canonical IR changes.
- The new canonical IR has a new CID.
- The old memento (against the old CID) is not applicable.
- The new function needs new mementos.

Monotonicity doesn't update mementos to new contracts. It just doesn't invalidate old ones. The old mementos describe the old behavior of the old function; they remain accurate for the old function. They have nothing to say about the new function.

### Old mementos can be misleading if used out of context

A consumer who has an old memento and uses it against new code is doing something wrong: the memento was minted for old code; using it for new code is a category error. The protocol's pinning (CID-based references) prevents this: the memento references specific CIDs; consumers must reference the same CIDs.

### Trust decay doesn't auto-expire

A memento signed by a compromised key remains valid against the bytes. A trust-set update rejects it; the trust-set update is the operator's responsibility. No automatic expiry.

This is intentional. Automatic expiry would be a centralized state-mutation, defeating the protocol's design. The right model is: each operator's local trust decision; the lattice is unchanged.

### Forwards reasoning, not backwards

Monotonicity is "old facts stay true." It is not "new facts can be derived from old facts automatically." If you mint a new contract, you mint a new memento; the lattice doesn't auto-derive new mementos from old ones (except via the bridge composition, which is explicit).

## "Software ages backwards"

The phrase: every codebase that adopts Sugar becomes easier to verify than the one shipped yesterday.

What it means concretely:

- Today's lattice has all of yesterday's discharged pairs.
- Plus new pairs minted today.
- A codebase running today's verifier hits Tier 1 on yesterday's pairs.
- And on more pairs, because the lattice is bigger.

Tomorrow's lattice has all of today's pairs plus new ones. A codebase running tomorrow's verifier hits Tier 1 on more pairs than today's verifier.

Verification cost can decrease over time as more obligations hit prior CIDs or
cached implication mementos instead of requiring new semantic proof.

This is the inverse of conventional software, where dependencies get harder to reason about as the dependency tree grows. With Sugar, the dependency tree's verification cost decreases as the lattice grows. The codebase ages "backwards" in the sense that older codebases are harder to verify (less lattice support) than newer ones.

## What this argues for

The monotonic-provability property argues for:

### 1. Aggressive minting

Don't worry about minting "too many" implication mementos. The lattice can absorb arbitrarily many; storage is the only cost; storage is cheap. Mint everything that gets discharged.

### 2. Sharing aggressively

The implication-server pattern (a passive indexer that aggregates mementos from many parties) maximizes amortization. Every party's discharge can become every other party's Tier 2 hit. Sharing is positive-sum.

### 3. Pinning, not naming

References between mementos are by CID, not by name. Names can be remapped; CIDs cannot. The protocol's references are stable forever (against specific bytes).

### 4. Content-driven discovery

Indexers should query by CID. Name-to-CID resolution is a convenience layer, not the source of truth. Operators who run their own indexers should serve CID-keyed lookups primarily.

### 5. No "version compatibility" complexity

Old mementos work with new verifiers; new verifiers work with old mementos (where the IR primitives haven't changed). No version pinning required for the lattice; only for the protocol catalog (which is itself content-addressed).

## Tracing back to the math

Monotonicity is a structural property: it follows from content-addressing + cryptographic signatures + lattice tractability. There's no "monotonicity feature" added on top; it's emergent from the core design choices.

The protocol's correctness theorems (in `protocol/specs/`) include monotonicity as a corollary. See [`../reference/lattice-tractability.md`](../reference/lattice-tractability.md) (when written) for the formal statement.

## Read next

- [content-addressing-not-registry.md](content-addressing-not-registry.md): the primitive monotonicity is built on.
- [cross-domain-verification.md](cross-domain-verification.md): how monotonicity makes cross-language transfer permanent.
- [thesis.md](thesis.md): the full claim.
- [cold-start.md](cold-start.md): how monotonicity affects bootstrap.
