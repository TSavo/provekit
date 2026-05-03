# Reference contracts

Reference contracts are the curated bridge anchors that make cross-language verification work. Without them, two implementations of `parseInt` in two different languages have nothing in common; with them, both bridge to the same canonical target and Tier 1 discharge becomes possible.

This directory is the curated registry. Each reference contract has its own page documenting what it is, what it claims, and how implementations bridge to it.

## What a reference contract is

A reference contract is a canonical, content-addressed contract memento describing a precise behavioral specification. The CID identifies the contract; multiple implementations across languages bridge to that CID; the handshake walks the bridges.

Concretely:

- A reference contract has a fixed canonical IR.
- Its CID is the BLAKE3-512 of the canonical bytes.
- It is signed by a foundation key (or quorum) for the reference-contracts authority.
- Its existence and identity are pinned in the protocol catalog.
- Implementations across languages bridge to it.

The reference contract is the lingua franca. Each implementation's bridge says: "my contract implies the reference contract." Once enough implementations bridge in, cross-language consumers get Tier 1 discharge for free.

## Why reference contracts matter

Without reference contracts:

- Each implementation has its own contract.
- Cross-language consumers cannot inherit verification across implementations.
- Each consumer re-verifies independently.

With reference contracts:

- Implementations share a common target.
- Consumers' pre-conditions match the reference (because they're canonical).
- Bridges connect everything.
- Tier 1 fires.

Reference contracts are the bootstrap accelerant for the protocol. See [`../explanation/cold-start.md`](../explanation/cold-start.md) and [`../explanation/cross-domain-verification.md`](../explanation/cross-domain-verification.md).

## What's in this directory

Each reference contract has its own `.md` documentation page in this directory:

- [`ref-parseInt-v1.md`](ref-parseInt-v1.md) — ECMA-262 `parseInt` semantics
- [`ref-parseFloat-v1.md`](ref-parseFloat-v1.md) — ECMA-262 `parseFloat` semantics
- [`ref-email-format-v1.md`](ref-email-format-v1.md) — RFC 5322 email format validation
- [`ref-uuid-v1.md`](ref-uuid-v1.md) — RFC 4122 UUID format
- [`ref-iso8601-date-v1.md`](ref-iso8601-date-v1.md) — ISO 8601 date format
- [`ref-ip-address-v1.md`](ref-ip-address-v1.md) — IP address validation
- [`ref-uint32-arithmetic-v1.md`](ref-uint32-arithmetic-v1.md) — 32-bit unsigned arithmetic
- [`ref-int32-arithmetic-v1.md`](ref-int32-arithmetic-v1.md) — 32-bit signed arithmetic
- [`ref-malloc-v1.md`](ref-malloc-v1.md) — POSIX `malloc` semantics
- [`ref-ieee754-arithmetic-v1.md`](ref-ieee754-arithmetic-v1.md) — IEEE-754 floating-point arithmetic

Each page documents:

- The CID of the contract memento.
- The canonical IR formula.
- The semantics in prose (what it claims).
- Implementations that bridge to it.
- Known limitations or scope.

The actual canonical bytes live in `protocol/reference-contracts/` (when populated). The pages here are the human-readable indexes.

## How to use a reference contract

If you're an implementation author:

1. Find the relevant reference contract (or propose a new one if none exists).
2. Lift your implementation's annotations to canonical IR.
3. Discharge "my contract implies the reference contract" via your kit's prover backend (or contributed evidence from a different solver).
4. Mint a bridge memento with the implication evidence.
5. Sign and publish.

Now consumers in any language whose pre-conditions match the reference contract can discharge against your implementation at Tier 1.

If you're a consumer:

1. Identify which reference contract applies to a call site (e.g., your `parseInt` call site applies `ref-parseInt-v1`).
2. Express your pre-condition in canonical IR matching the reference. (Most often, the reference IS your pre-condition; canonical references are designed for this.)
3. Run `provekit prove`. The handshake walks bridges from the implementation's contract to the reference, discharging at Tier 1 when CIDs match.

## How a reference contract is added

Reference contracts have governance distinct from kits and adapters. Adding a reference contract is a deliberate, reviewed action because:

- Multiple implementations rely on the canonical IR.
- Drift in the canonical IR breaks every implementation that bridged in.
- The reference is "official" in a way that other mementos aren't.

The proposal process:

1. **Identify the gap.** A common call site exists across languages with no shared reference contract. Examples to follow: parseInt, email, UUID, etc.
2. **Draft the canonical IR.** The full formula in canonical form. Include rationale: why this canonical IR captures the semantics correctly.
3. **Open a proposal PR** under `protocol/reference-contracts/proposals/`. Include the canonical IR, the rationale, the proposed CID (deterministic from the IR), the documentation page (this directory).
4. **Review.** Reference-contract maintainers review for: semantic accuracy, scope (is this too broad / narrow), ergonomic fit (will implementations actually bridge here).
5. **Mint and pin.** On approval, the canonical IR is committed, the CID is recorded, the documentation page is committed, and the protocol catalog is updated to reference the new contract.
6. **Implementation bridges follow.** Implementation authors update their kits to bridge against the new reference. The lattice grows.

Reference-contract maintainers are a separate group from kit maintainers (though there is overlap). Their job is curation: ensuring the canonical IR set is coherent, semantically accurate, and ergonomically useful.

## What's in scope

Reference contracts cover behavioral semantics that are widely shared across language ecosystems:

- **Parsing** of structured data: numbers, dates, UUIDs, IPs, URLs.
- **Format validation**: emails, JSON shapes, regex patterns.
- **Arithmetic semantics**: int32, uint32, int64, IEEE-754.
- **Standard library functions**: `malloc`, `free`, `strcmp`, `memcpy` (where the semantics are well-specified).
- **Cryptographic primitives**: hash function inputs/outputs, signature scheme inputs/outputs.

Each of these has a canonical specification (RFC, language standard, IEEE/ISO/W3C document). The reference contract captures the spec in canonical IR form.

## What's out of scope

- **Application-specific behavior**: "my company's User schema has these fields." Not generic enough to be a reference; covered by application-level contracts.
- **Highly-typed library APIs**: e.g., `lodash.merge` semantics. Library-specific; covered by per-library `.proof` files, not the reference set.
- **Implementation details**: e.g., a specific hash function's bit-level constant; that's the function's spec, not a reference.

## Versioning

Reference contracts are versioned by suffix: `ref-parseInt-v1`, `ref-parseInt-v2`, etc. A v2 is a different memento with a different CID; existing implementations bridge to v1 and continue to work; new implementations may choose v1 or v2.

Bumping versions is rare and conservative. Reasons:

- The original spec was wrong (very rare).
- The relevant external standard updated (e.g., a new ECMA-262 edition).
- A widely-adopted refinement of semantics (e.g., adding leading-whitespace handling).

When a v2 is added, the v1 is not removed. Both remain reachable; old implementations stay valid.

## Foundation key

Reference contracts are signed with a foundation key dedicated to reference-contract authoritarian. This key is distinct from:

- The protocol catalog foundation key (used for self-contracts mints).
- Individual developer keys (used for personal contracts).
- Prover backend keys (used for implication mementos).

The reference-contract foundation key's public key is part of the protocol catalog. Verifiers configured to trust reference contracts trust this key.

For high-assurance deployments, multiple foundation keys may be required: a reference contract is signed by N-of-M keys held by independent maintainers.

## Maintenance

Reference contracts are maintained by:

- A small group of maintainers with commit access to `protocol/reference-contracts/`.
- A review process for proposed additions.
- Ongoing review for clarifications, refinements, version bumps.

Contributing a new reference contract or refining an existing one is the highest-leverage contribution to the ecosystem: every implementation that bridges in benefits.

## Read next

- [`ref-parseInt-v1.md`](ref-parseInt-v1.md) — the canonical example.
- [`../explanation/cross-domain-verification.md`](../explanation/cross-domain-verification.md) — the mechanism reference contracts enable.
- [`../explanation/cold-start.md`](../explanation/cold-start.md) — why reference contracts accelerate adoption.
- [`../contributing/proposing-a-spec-change.md`](../contributing/proposing-a-spec-change.md) — when adding a reference requires a spec change.
- [`../tutorials/polyglot-stack.md`](../tutorials/polyglot-stack.md) — the worked cross-language demo.
