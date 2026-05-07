# Canonical Form

Canonical form is the byte representation that ProvekIt hashes, signs, and compares. A verifier never hashes an in-memory object. It hashes the canonical bytes emitted from that object.

## Rule

For JSON-shaped protocol bodies and ProofIR values:

1. Serialize the value with JCS.
2. Hash the exact emitted bytes with BLAKE3-512.
3. Prefix the 64-byte digest as `blake3-512:<128 lowercase hex chars>`.

The CID is a name for those bytes, not for the pretty-printed JSON, host-language struct, AST node, or source annotation that produced them.

## Practical Checks

- Objects sort keys according to JCS.
- Numbers, strings, booleans, nulls, arrays, and objects use the JSON canonicalization rules in the cataloged grammar.
- Producers and consumers compare bytes before diagnosing semantic disagreement.
- CIDs are recomputed from bytes at trust boundaries.
- Signed payloads exclude only the fields explicitly excluded by their protocol spec.

## Common Mistakes

**Hashing a structure.** Host-language object layout is not canonical. Serialize first, hash second.

**Hashing pretty JSON.** Whitespace and key order are presentation details. Reparse and emit JCS bytes.

**Mixing digest formats.** The current protocol uses full BLAKE3-512 CIDs for these claims. Short hashes are display-only.

**Treating equal syntax as equal semantics.** Cross-domain equivalence exists only after both sides lift to the same canonical claim or an accepted bridge connects them.

## Normative Specs

- [Canonicalization grammar](../../../protocol/specs/2026-04-30-canonicalization-grammar.md).
- [IR formal grammar](../../../protocol/specs/2026-04-30-ir-formal-grammar.md).
- [Current CID registry](../cids.md).
