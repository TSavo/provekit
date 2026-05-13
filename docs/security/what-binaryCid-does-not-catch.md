# What `binaryCid` does NOT catch

The `binaryCid` field is a strong supply-chain anchor (see [`what-binaryCid-catches.md`](what-binaryCid-catches.md)). It is not a complete defense. This document is the explicit list of attacks `binaryCid` does not detect.

## Lying contracts paired with matching binaries (caught at the rank-3 level)

Pre-v1.4, this was the most important not-catch from `binaryCid` alone. The attack: write a malicious function that does X, sign a contract claiming Y, ship the malicious binary so `binaryCid` matches, lie about the rest.

Under v1.4's rank-3 pin, this attack class **is caught, by the witness axis, not by `binaryCid`**. The consumer pins `contractCid` separately (signer-independent, content-only) and requires a `witnessCid` from a prover key in their trust set attesting `post → pre` for that contract. A lying signer cannot produce a valid witness without controlling a trusted prover key.

`binaryCid`'s role here is partial: it ensures the binary you run is the binary the witness was minted against. The witness axis ensures the contract is what it claims to be. The contract axis ensures the contract identity is stable across re-attestation. Together, the three axes close this attack class.

What `binaryCid` alone does not catch is the *binary-only* slice of the attack: an honest contract paired with a substituted binary. That `binaryCid` does catch.

See [`multi-dimensional-pinning.md`](multi-dimensional-pinning.md) for the rank-3 architecture and [`threat-model.md`](threat-model.md) for the updated threat matrix.

## Source-level changes that compile to identical bytes

Some source changes don't affect compiled output:

- A reordering of source code that the compiler normalizes during AST construction.
- A semantic comment change that the compiler doesn't process.
- A trailing whitespace change.
- A dead-code branch that the optimizer eliminates.

If an attacker can find a source change that doesn't affect compiled bytes but affects the signer's claim about behavior, `binaryCid` doesn't detect it. This is rare but not impossible.

In practice: this attack is most viable when the contract's claim depends on source-level structure that the compiler erases. Most contracts are about runtime behavior, which the compiler preserves; this attack vector is narrow.

Mitigation: contracts should claim runtime behavior, not source structure.

## Compiler backdoors injected before proof mint

If the compiler is backdoored, and the backdoor is in place when the developer mints the `.proof`, then:

1. The developer compiles their honest source.
2. The compiler produces a malicious binary (silently, by design).
3. The developer's verifier (using the same backdoored compiler) verifies the binary against the contract.
4. The compiler's backdoor causes the verifier to incorrectly approve the malicious behavior.
5. The developer signs the `.proof`. The proof pins the malicious binary's CID.
6. Consumers receive the proof, find the binary's hash matches `binaryCid`, accept.

The whole pipeline is poisoned. The proof is internally consistent (binary matches CID) and externally signed by an honest developer who didn't realize their compiler was backdoored. The verifier sees no anomaly.

This is the residue of Thompson's "Trusting Trust" attack within ProvekIt. The defense is bootstrappable systems and reproducible builds (cross-compiling with multiple compiler vendors, comparing outputs). ProvekIt does not solve Trusting Trust; it complements solutions that do.

## Compromise of the developer's signing key

If an attacker steals a developer's signing key, they can mint signed contract mementos that the verifier accepts as trusted, including ones that mis-state behavior. `binaryCid` checks the binary against the contract's claim, but the contract was forged by the attacker. The mismatch is hidden by the attacker's free hand to choose both the binary and the contract.

Mitigations:

- Hardware-key signing (HSM, hardware tokens, secure enclaves) reduces key-extraction risk.
- Quorum signing (require N-of-M signatures) raises the bar.
- Revocation lists distributed out-of-band let consumers reject keys after compromise is detected.

The protocol does not prescribe these. They are operational practices around the protocol.

## Runtime modifications after verification

`provekit prove` runs at build time (or at deploy time). Once verification passes and the binary is loaded, the runtime is out of the protocol's scope:

- In-memory code injection (process exploitation).
- Just-in-time compilation that produces different machine code than the verifier saw.
- Dynamic loader interception that loads a different module than the verifier verified.
- Hot-reloading that swaps in modified code.

These attacks happen after verification. `binaryCid` says "the binary you loaded matches the proof"; it doesn't say "the binary you're running right now hasn't been modified since you loaded it."

Mitigations: runtime integrity monitoring (Linux IMA, Windows Code Integrity), hardware-based attestation (TPM, Intel SGX, ARM TrustZone), execution-time integrity checks. These are systems-level defenses, complementary to ProvekIt.

## Substitution of indirect dependencies

`binaryCid` pins the artifact directly mentioned in the `.proof`. It does not directly pin the artifacts that the directly-mentioned artifact loads at runtime.

Scenario: package A's `.proof` pins A's binary. A loads B at runtime via `dlopen` / `import` / `require`. The attacker substitutes B's binary. A's `binaryCid` matches; B's substitution is undetected.

Mitigation: B should ship its own `.proof` with its own `binaryCid`. The verifier should walk the full dependency graph, verifying each artifact's `binaryCid` against its `.proof`. Today, transitive verification is in flight; v1.x kits typically verify one artifact at a time.

For now, the consumer must explicitly verify each dependency, not just the top-level artifact.

## Side-channel attacks

A binary that satisfies its behavioral contract may still leak information through timing, power consumption, electromagnetic emanation, cache access patterns, or other side channels.

Behavioral contracts are about input-output relations. Side channels are non-functional properties. The protocol does not capture them.

Mitigation: side-channel analysis is a separate discipline. Cryptographic libraries publish constant-time guarantees outside ProvekIt's scope. ProvekIt complements but does not replace.

## Resource exhaustion

A contract can claim "this function returns the correct result" without claiming "this function returns in bounded time" or "this function uses bounded memory." A malicious dependency can satisfy its behavioral contract while exhausting the consumer's resources.

Mitigation: resource bounds need their own contracts and their own canonical predicates. Some adapters (proptest, hypothesis) can lift bounded-iteration constraints; most don't.

## Sandbox escapes

A binary that satisfies its behavioral contract may exploit a runtime sandbox bug to escape its intended permissions. The contract describes intended behavior; the exploit is unintended behavior outside the formal model.

Mitigation: sandboxes need their own integrity. ProvekIt does not validate sandbox correctness.

## Time-of-check vs. time-of-use

The verifier computes the binary's hash at one point in time; the binary executes later. If the binary is modified between these two points, the modification is undetected.

For desktop applications: the gap is short (milliseconds between load and start of execution); the attack surface is small.

For long-running servers: the gap is large (months between deploy and re-verification). An attacker who can patch a running process has a long window.

Mitigation: re-verify periodically. Hardware-backed runtime integrity monitoring catches in-process modification.

## Implicit dependencies (configuration, environment, OS state)

The contract describes the function's behavior in isolation. Real functions depend on environment: configuration files, environment variables, OS state, locale, time zone, random seeds, hardware capabilities.

If the contract's IR doesn't capture the environmental dependency, a function that satisfies its contract in one environment may fail in another. `binaryCid` doesn't help; the binary is the same; the environment differs.

Mitigation: contracts should be explicit about environmental assumptions (treat configuration as input). This is per-contract authoring discipline.

## Summary

`binaryCid` catches: tampering with the binary itself, between proof mint and consumer load.

`binaryCid` does NOT catch:

- Lying signers.
- Source-level changes that don't affect compiled bytes.
- Compiler backdoors active during proof mint.
- Signing key compromise.
- Runtime modifications after load.
- Indirect dependency substitution.
- Side channels.
- Resource exhaustion.
- Sandbox escapes.
- Time-of-check / time-of-use gaps.
- Environmental dependencies.

The first item is the most consequential. `binaryCid` plus tampering detection plus per-member CID checks closes a real and large slice of supply-chain risk. The remaining items require complementary defenses; ProvekIt explicitly does not pretend to address them.

## Defense-in-depth posture

ProvekIt is one layer. The realistic security posture for a project that uses ProvekIt:

1. **Source review**: does the source say what we think it says?
2. **Reproducible builds**: does the source compile to the same bytes everywhere?
3. **`binaryCid` pinning**: is the binary I'm running the binary the signer intended?
4. **Behavioral contracts**: does the binary's behavior match what's claimed?
5. **Runtime integrity**: is the running process unchanged from what was loaded?
6. **Network and sandbox isolation**: what can the binary affect even if it's malicious?
7. **Audit logging**: what did the program actually do at runtime?

ProvekIt covers (3) and (4). Other layers cover (1), (2), (5), (6), (7).

## Read next

- [what-binaryCid-catches.md](what-binaryCid-catches.md): the catches.
- [threat-model.md](threat-model.md): the full threat coverage matrix.
- [supply-chain.md](supply-chain.md): supply-chain scenarios.
- [solver-trust.md](solver-trust.md): Z3 as TCB.
- [adapter-trust.md](adapter-trust.md): lift adapters as TCB.
