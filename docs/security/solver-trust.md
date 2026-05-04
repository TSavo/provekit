# Solver trust: Z3 (and others) as TCB

ProvekIt is a protocol for content-addressing formal verifications. The verifications themselves come from solvers. The protocol does not endorse any solver's correctness; it transports the solver's output as portable, signed evidence.

This means solver trust is a load-bearing user decision. This doc walks the implications.

## What the verifier actually trusts

Three layers of trust, in order of TCB depth:

### 1. The protocol's primitives

BLAKE3-512 is collision-resistant. Ed25519 signatures are unforgeable. JCS canonicalization is deterministic. The verifier's TCB at this layer is the cryptographic primitives plus the verifier's implementation of them.

This layer's TCB is small, well-audited, and shared with most security-critical software (similar primitives underlie TLS, SSH, the major package signing schemes).

### 2. The kit's conformance to the protocol

The kit canonicalizes IR formulas to bytes; the verifier trusts the bytes. If the kit's canonicalizer produces wrong bytes, the resulting CIDs are wrong and the verifier's discharge is wrong.

The conformance harness (`make conformance`) is the defense. Every kit's bytes are byte-equality-checked against the protocol's canonical bytes via fixtures and self-contracts mints. The harness is the user's audit hook for "is this kit doing the right thing?"

### 3. The solver

When Tier 3 fires, the verifier invokes a solver (Z3 by default; CBMC, Lean, Coq, F\* if configured). The solver returns `unsat` (claim holds) or `sat` (counterexample exists). The verifier mints an implication memento containing the solver's output.

If the solver is wrong, the memento codifies the wrong result. Every future verifier discharges against the wrong memento. The cache poisoning is permanent.

This is the deepest TCB layer. The protocol does not validate the solver. It trusts whatever the solver says and signs the result.

## Z3 specifically

Z3 is the default Tier 3 backend for v1.x. Why Z3:

- Broad theory support (linear arithmetic, bit-vectors, arrays, strings, uninterpreted functions, quantifiers).
- Fast for typical contract sizes (sub-second for most pairs).
- Mature: 20+ years of development, used in deployed verification tools.
- SMT-LIB standard input format (other solvers can substitute).
- Apache 2.0 licensed.

Z3's known issues:

- **Quantifier instantiation heuristics.** Z3 uses pattern-based instantiation; pathological formulas can cause non-termination or timeouts. Tier 3 has a configurable timeout; on timeout, the verifier falls back to per-call-site flagging.
- **Soundness bugs.** Z3 has had soundness bugs in the past (rare; well-tracked). A `unsat` from a buggy Z3 version is wrong; the protocol does not detect this.
- **Floating-point arithmetic.** Z3's float semantics are well-defined but counter-intuitive. Floating-point contracts are easy to express incorrectly.

For most contract pairs in most codebases, Z3 is the right TCB. For high-stakes contracts (cryptographic primitives, kernel code, life-safety systems), users should require multiple backend concurrence or escalate to a constructive-proof backend.

## Multi-backend concurrence

The protocol supports multiple Tier 3 backends. Users configure trusted backends in `provekit.config.yaml`:

```yaml
trusted_provers:
  - "z3-foundation-ed25519"
  - "cbmc-foundation-ed25519"

require_concurrence:
  for_modules: ["high-stakes/**"]
  min_provers: 2
```

When `require_concurrence` is set for a code path, the verifier requires N independent backends to all return `unsat` before discharging the call site. The implication memento contains evidence from all N; the verifier checks all N signatures and all N evidence terms.

This raises the TCB bar substantially: a single backend bug doesn't poison the cache; the bug would have to exist in all N backends simultaneously and produce identical wrong evidence.

For most users, single-backend (Z3) is adequate. For users with elevated trust requirements, multi-backend concurrence is a configurable upgrade.

## Constructive-proof backends

Z3 returns `unsat` heuristically. It does not produce a constructive proof (a proof term verifiable by a separate checker). For high-assurance domains, this is unsatisfactory.

Constructive-proof backends produce verifiable proof terms:

- **Coq**: Coq tactic scripts produce proof terms; the kernel checks them. The kernel TCB is roughly 10kloc of well-audited OCaml.
- **Lean**: Similar shape; Lean's kernel TCB is comparable.
- **F\***: Compiles to verifiable F\* proofs.
- **Isabelle**: Higher-order logic with a small TCB.

A constructive-proof backend is much slower than Z3 but produces evidence that an independent checker can re-verify. The protocol stores the evidence; users can re-run the kernel against the evidence offline to validate the discharge.

This is the path for users who reject Z3 as TCB. It costs CPU time; it buys assurance.

## The signature is what's portable

The protocol's role is to make solver evidence portable. A discharge mements is:

```
{
  "post_cid": "blake3-512:bafy...post-v1",
  "pre_cid":  "blake3-512:bafy...pre-v1",
  "evidence": <prover-specific evidence term>,
  "prover_kind": "z3-v4.13.0",
  "publicKey": <signer key>,
  "signature": <Ed25519 over the above>
}
```

Anyone with the memento and the signer's public key can:

1. Verify the signature.
2. Re-run the prover (or a different prover) on `(post, pre)` and compare against the evidence.
3. Decide whether to trust the discharge.

The signature is non-repudiable: the signer cannot deny having claimed the discharge. The evidence is auditable: anyone can re-verify offline.

The protocol's value-add is that this audit can happen *anywhere*, by anyone, asynchronously. The mement is content-addressed; consumers fetch it; trust decisions are local.

## Trust transitivity

A consumer trusts a memento only if they trust the signer. The signer might trust their kit; the kit might trust the canonicalizer; the canonicalizer might trust the JCS implementation; the verifier might trust the prover.

Each link in this chain is a trust decision. ProvekIt makes the chain visible: every step is signed, every step is auditable, every step's evidence is portable.

This is more trust than a typical "I ran tests" claim provides. It is less trust than a full Coq certificate provides. The protocol is in the middle: a substrate over which trust decisions are made, not a guarantee of trust.

## What the protocol does NOT do

- Verify the solver's correctness.
- Detect solver bugs after the fact.
- Provide a constructive proof of soundness.
- Substitute for backend-level audit.

What the protocol does:

- Make solver outputs portable.
- Make solver outputs signed and tamper-evident.
- Make solver outputs reusable across the dependency graph.
- Provide a substrate for users to make their own trust decisions.

If you read "ProvekIt verifies your code" as "ProvekIt mathematically proves your code is correct," you've misread. The mathematical proof comes from the solver. ProvekIt is the protocol that makes the solver's proof distributable and composable.

## Operational recommendation

For most users:

1. **Trust Z3** for general-purpose contracts. It's the right cost/value point.
2. **Configure timeouts** at Tier 3 (default 30s). Higher timeouts trade build time for fewer per-call-site fallbacks.
3. **Watch for soundness advisories** about your Z3 version. If a soundness bug is published, evict cache mementos that depended on the affected version.

For high-stakes users:

4. **Require multi-backend concurrence** for security-critical code paths.
5. **Audit constructive-proof evidence** offline for your most sensitive contracts.
6. **Pin solver versions** in your build environment so a newer (potentially buggier) solver doesn't silently change discharge behavior.

For everyone:

7. **Read the boundaries.** The signature attests to the signer; it does not attest to truth. See [`../explanation/boundaries.md`](../explanation/boundaries.md).

## Read next

- [adapter-trust.md](adapter-trust.md): lift adapters as TCB.
- [signature-and-non-repudiation.md](signature-and-non-repudiation.md): what signatures buy.
- [threat-model.md](threat-model.md): full threat coverage.
- [`../contributing/writing-a-prover-backend.md`](../contributing/writing-a-prover-backend.md): adding backends.
- [`../explanation/compared-to/coq-fstar-lean.md`](../explanation/compared-to/coq-fstar-lean.md) (when written): interactive theorem provers.
