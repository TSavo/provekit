# Threat model

What attacks does Sugar actually catch? What does it not? This document is the explicit threat model. It is precise about what the protocol detects, mathematically; it is precise about what falls outside its scope.

> **v1.4 update.** Multi-dimensional pinning (the rank-3 consumer pin: `(contractCid, witnessCid, binaryCid)`) closes attack classes that were structurally open under v1.1's single-CID pinning. The "lying contracts paired with matching binaries" attack class (previously listed as the most important non-catch) is closed at the rank-3 level. See [`multi-dimensional-pinning.md`](multi-dimensional-pinning.md) for the architecture; the table below has been updated accordingly.

## The asset

A consumer's running program, including its dependency closure. Specifically: the bytes of every compiled artifact loaded into the program, the behavior of every function the program calls, and the contracts the consumer relies on those functions to satisfy.

The asset is broader than just the consumer's source code. It includes the entire transitive dependency graph and the binaries those dependencies compile to.

## The adversary

A wide range. From least to most powerful:

1. **Honest-mistake attacker.** A library author who introduces a bug. Same as today's threat: code reviews, tests, and the user's verifier are the defenses.
2. **Compromised dependency.** An npm package whose maintainer's account was compromised, or a Rust crate with a malicious commit merged.
3. **Supply chain insertion.** An attacker who controls the registry (npm, crates.io, PyPI) and serves a tampered package to specific users.
4. **Build-time substitution.** An attacker who controls the user's CI or build environment and substitutes a tampered binary.
5. **Compiler backdoor.** An attacker who controls the compiler. Reflections on Trusting Trust (Thompson 1984).
6. **Runtime patching.** An attacker who controls the runtime (JIT, dynamic loader) and modifies code after the verifier has run.

Different threat tiers; different defenses; different Sugar coverage.

## Catches: tampering with `.proof` content

A `.proof` bundle's CID is BLAKE3-512 of its bytes. Any modification to the bundle changes the CID. The filename is the CID. The verifier rejects any bundle whose recomputed CID differs from its filename.

Attacker action: edit a `.proof` to remove a bridge or modify a contract's pre-condition.
Detection: verifier sees CID mismatch, rejects bundle.
Effective threat tiers: 2, 3, 4 (any attacker who can serve a modified bundle).
Not effective against: 5, 6 (attackers below the verifier's TCB).

## Catches: tampering with individual mementos inside a bundle

Each member's CID is BLAKE3-512 of its bytes. The bundle's `members` map keys those CIDs. The verifier checks per-member CID consistency.

Attacker action: replace a contract memento inside a bundle with a different contract.
Detection: per-member CID mismatch.
Effective threat tiers: 2-4.

## Catches: forgery of new contracts under a key the attacker doesn't hold

Each contract memento is signed with Ed25519. The signature covers the inner bytes (the canonical IR). An attacker without the private key cannot produce a valid signature.

Attacker action: mint a new contract memento claiming to be from a trusted developer.
Detection: signature verification fails.
Effective threat tiers: 2-4 (assuming key remains uncompromised).
Not effective against: attackers who compromise the developer's signing key.

## Catches: stale `.proof` against current binary

When `binaryCid` is set, the verifier compares it against the running compiled artifact's hash. Any difference rejects the proof.

Attacker action: serve old `.proof` bundle paired with a tampered new binary.
Detection: `binaryCid` mismatch.
Effective threat tiers: 2-4.

## Catches: cross-cutting dependency injection

If an attacker controls a runtime DI container or monkey-patches a module to redirect calls, the contract bound to the call site doesn't match the bound function. The bridge's `boundCallSiteSymbol` resolves differently than the bridge expected.

Attacker action: monkey-patch `parseInt` at runtime to redirect to a malicious implementation.
Detection: the runtime check that the active `parseInt` matches the contract's bound `contractCid` (the bridge references the contract by its content-only CID per `2026-05-03-contract-cid-vs-attestation-cid.md` R3) fails.
Effective threat tiers: 4-6 (including some runtime patching).

This catch depends on the kit's runtime guard implementation. Not all kits have it yet; check the per-language status matrix.

## Catches under rank-3 pinning: signed contracts that mis-state behavior

Under v1.4's rank-3 consumer pin `(contractCid, witnessCid, binaryCid)`, the "lying contracts paired with matching binaries" attack class is closed. The mechanism:

- The consumer pins a `contractCid` (content-only; signer-independent).
- The consumer requires a witness whose signature is from a key in their trust set, attesting `post → pre` for that contract.
- The consumer checks `binaryCid` against the running artifact's hash.

To attack, an adversary must coherently substitute all three:

1. A forged binary (changes `binaryCid`).
2. A forged contract that the new binary actually satisfies (changes `contractCid`).
3. A witness signed by a key the consumer trusts. **This is the irreducible bar.**

Attacker action: write a function that does X, sign a contract claiming the function does Y.
Detection under v1.4 rank-3: the consumer's pin includes `witnessCid`. The attacker's signed-but-mis-stated contract has no witness chain rooted in a trusted prover; the attacker would need to compromise a trusted prover's signing key to produce a fraudulent witness. **This is a key-compromise attack, not a contract-substitution attack.**

What remains: if the attacker compromises a trusted prover key, they can mint witnesses for any contract / binary pair. Mitigation: hardware-key signing for prover keys; quorum signing requiring N-of-M attestations; revocation lists. The protocol provides hooks for these; the operator chooses their threat model.

Pre-v1.4, this attack class was structurally not caught; single-CID pinning collapsed the `(contract, witness, binary)` tuple onto a single CID. v1.4's rank-3 framing makes the attack class catchable. See [`multi-dimensional-pinning.md`](multi-dimensional-pinning.md).

## Does NOT catch: solver bugs

Tier 3 invokes Z3 (or another configured backend). If Z3 returns `unsat` incorrectly (i.e., declares the implication holds when it doesn't), the verifier mints an implication memento codifying the wrong result. Every future verifier discharges against the wrong memento.

Attacker action: trigger a Z3 bug to cause a wrong `unsat`.
Detection: NONE within the protocol unless the implication is independently re-verified by another solver.

Mitigation: configure multiple trusted prover backends. If Z3 and CBMC both discharge the same pair, the chance of both being wrong on the same pair is low. The protocol supports multiple backends; users can require concurrence.

This is the "Z3 as TCB" issue. See [`solver-trust.md`](solver-trust.md).

## Does NOT catch: lift adapter mis-translation

A lift adapter walks a source library's annotations and produces canonical IR. If the adapter mis-translates an annotation, the canonical IR doesn't reflect what the annotation actually means. Subsequent contracts derived from the wrong IR are wrong.

Attacker action: pick an annotation library whose adapter has a known mis-translation; rely on the wrong canonical IR being trusted.
Detection: NONE within the protocol. Detection requires per-adapter audit.

Mitigation: adapter conformance fixtures (in `tests/adapter-fixtures/`) catch known mis-translations. Cross-adapter parity tests catch divergence between adapters that should agree. Neither is a complete defense.

This is the "adapter trust" issue. See [`adapter-trust.md`](adapter-trust.md).

## Does NOT catch: developer signing key compromise

If an attacker steals a developer's signing key, they can mint signed contract mementos that the verifier accepts as trusted.

Attacker action: phish a developer, exfiltrate signing key, mint malicious signed contracts.
Detection: NONE within the protocol unless the verifier has fresh revocation information.

Mitigation: hardware-key signing, quorum signing (require N-of-M signatures), revocation lists distributed out-of-band. The protocol does not implement these directly; they are operational practices around the protocol.

## Does NOT catch: compiler backdoors that reproduce in the binary

If the compiler is backdoored such that compiled binaries deviate from the source's claimed behavior, but the binary's CID is what `binaryCid` pins, then `binaryCid` matches the running artifact (which is what the user compiled). The contract doesn't match the artifact's actual behavior, but the verifier doesn't detect this.

This is Thompson's "Trusting Trust" attack. Sugar does not solve it. The defense is reproducible builds, multi-vendor compiler comparison, and bootstrappable systems (Bootstrappable.org's work). Sugar's `binaryCid` pins the artifact you ran; it doesn't validate the artifact's source.

## Does NOT catch: runtime patching after verification

If an attacker patches the running process's code after `sugar prove` has discharged the call sites, the verifier doesn't see the patch. The verification was honest; the runtime patches are out of scope.

Attacker action: in-memory code injection, JIT manipulation, dynamic loader interception.
Detection: NONE within the protocol; this requires runtime integrity monitoring (Linux IMA, Windows Code Integrity, hardware-based attestation).

Sugar is a build-time gate, not a runtime monitor. Runtime threats are out of scope.

## Does NOT catch: timing attacks, side channels, resource exhaustion

The protocol verifies behavioral contracts, not non-functional properties. A function that returns the correct value in time exponential in input size satisfies its behavioral contract; Sugar approves.

Out of scope:

- Timing attacks (information leakage via response timing).
- Side channels (cache, power, electromagnetic).
- Resource exhaustion (DoS via expensive inputs).
- Memory unsafety (use-after-free, buffer overflow) in languages where the contract doesn't capture memory state.
- Concurrency bugs (race conditions, deadlock) that don't manifest in the formula's universe.

These need different tools. Sugar complements them; it does not replace them.

## Does NOT catch: incorrect spec change

If a maintainer accepts a wrong spec change (e.g., adds a new IR primitive whose semantics are subtly off), every kit re-mints under the wrong semantics. Every contract minted under the new spec carries the wrong meaning.

Detection: NONE; the wrongness is structural, baked into the protocol.

Mitigation: rigorous spec review (see [`docs/contributing/proposing-a-spec-change.md`](../contributing/proposing-a-spec-change.md)), the social process of accepting changes only with strong motivation and clear semantics.

## Summary matrix (v1.4)

| Threat | Caught? | How |
|---|---|---|
| `.proof` content tampering | Yes | CID mismatch |
| Per-member tampering inside `.proof` | Yes | Per-member CID mismatch |
| Forging memento under unheld key | Yes | Signature failure |
| Stale `.proof` vs. tampered binary | Yes | `binaryCid` mismatch (axis 3 of rank-3 pin) |
| Signed-but-mis-stated contract paired with matching binary | **Yes (v1.4)** | Rank-3 pin: requires forged witness from trusted prover key |
| Re-attestation by different signers | **Yes (v1.4)** | `contractCid` is signer-independent; multi-witness DAG |
| Version downgrade attacks | **Yes (v1.4)** | `previousContractSetCid` chain validation |
| Shim poisoning / placeholder strings | **Yes (v1.4)** | `bridge-target-dimensionality` forbids placeholder strings; tagged-union targets |
| Runtime monkey-patching (kit-dependent) | Partial | Runtime guard if kit implements |
| Trusted prover key compromise | NO | Requires hardware-key + quorum + revocation; out-of-protocol |
| Solver bug (e.g., Z3 wrong `unsat`) | NO | Without multi-backend concurrence (configurable) |
| Lift adapter mis-translation | NO | Without adapter audit |
| Developer signing key compromise | NO | Without revocation |
| Compiler backdoor (Thompson) | NO | Out of scope; needs reproducible builds |
| Runtime patching after verification | NO | Out of scope; needs runtime integrity monitoring |
| Timing / side channel attacks | NO | Out of scope; not a behavioral property |
| Memory unsafety not captured in formula | NO | Out of scope unless contract captures it |

**Bold "Yes (v1.4)" entries** are attack classes closed by v1.4's multi-dimensional pinning that were structurally open under v1.1's single-CID framing.

## Defense in depth

Sugar is one layer in a defense-in-depth strategy. Other layers:

- Code review.
- Static analysis.
- Tests (unit, integration, fuzz).
- SBOM and dependency provenance (SLSA, Sigstore, in-toto).
- Reproducible builds.
- Runtime integrity monitoring.
- Hardware-rooted attestation.

Use them together. Sugar's strength is behavioral contracts as a content-addressed substrate; it does not replace any of the others.

## Read next

- [what-binaryCid-catches.md](what-binaryCid-catches.md): the supply-chain anchor in detail.
- [what-binaryCid-does-not-catch.md](what-binaryCid-does-not-catch.md): the limits of `binaryCid`.
- [solver-trust.md](solver-trust.md): Tier 3 backends as TCB.
- [adapter-trust.md](adapter-trust.md): lift adapters as TCB.
- [signature-and-non-repudiation.md](signature-and-non-repudiation.md): what the signatures buy.
- [supply-chain.md](supply-chain.md): supply-chain attack scenarios.
- [../explanation/boundaries.md](../explanation/boundaries.md): what Sugar is NOT.
