# Writing a prover backend

Tier 3 of the handshake invokes a solver to discharge `(post, pre)` pairs that aren't already in the cache. Today, the canonical Tier 3 backend is Z3. The protocol is solver-agnostic; alternative backends are explicitly in scope.

This doc is for contributors who want to add Lean, TLA+, CBMC, Coq, F\*, Isabelle, or another prover as an alternative Tier 3 backend.

## Why alternative backends

Different provers have different strengths:

- **Z3** (default): fast, broad theory support, SMT-LIB standard. Good for most behavioral contracts. The TCB is the Z3 binary.
- **CBMC**: bounded model checking for C/C++ semantics. Strong for memory-safety properties. Gives concrete counterexamples on `unsat` failures.
- **Lean / Coq / F\* / Isabelle**: full formal verification with constructive proofs. Highest assurance but slowest. Suitable for high-stakes domains (cryptography, OS kernels) where Z3's "trust the solver" posture is unacceptable.
- **TLA+**: temporal properties, refinement. Suitable for distributed-systems contracts.
- **Kani / Prusti / Creusot**: Rust-specific provers with high-quality Rust integration. Could be Tier 3 backends for Rust kits specifically.

Adding a backend lets users choose their TCB. Users who reject Z3 as untrustworthy can require Lean. Users who need bounded counterexamples can require CBMC.

## What a backend implements

A Tier 3 backend is a program that:

1. Reads a `(post, pre)` formula pair plus a context (sort declarations, axioms).
2. Attempts to prove `post implies pre`.
3. Returns one of: `proved` (with an evidence term), `disproved` (with a counterexample), `timeout`, or `inconclusive`.

The protocol does not interpret evidence terms; it stores them. A Lean backend produces Lean tactic scripts; a Coq backend produces Coq proof terms; a Z3 backend produces unsat cores. All are valid evidence per the protocol; verifiers configured to trust Lean require Lean evidence; verifiers configured to trust Z3 accept Z3 evidence; etc.

## The interface

A backend is a subprocess invoked by the kit:

```
Stdin:
  {
    "kind": "prove_request",
    "post_cid": "blake3-512:bafy...post-v1",
    "pre_cid":  "blake3-512:bafy...pre-v1",
    "post_formula": <canonical IR>,
    "pre_formula":  <canonical IR>,
    "context": {
      "sorts": [...],
      "axioms": [...]
    },
    "timeout_ms": 30000
  }

Stdout (success):
  {
    "kind": "prove_response",
    "result": "proved",
    "evidence": <evidence term, prover-specific shape>,
    "proof_time_ms": 234
  }

Stdout (failure):
  {
    "kind": "prove_response",
    "result": "disproved",
    "counterexample": {...},
    "proof_time_ms": 1402
  }

Stdout (timeout):
  {
    "kind": "prove_response",
    "result": "timeout",
    "elapsed_ms": 30000
  }
```

The backend is stateless across invocations. The kit invokes it once per `(post, pre)` pair. Caching is the kit's responsibility; on `proved`, the kit mints an implication memento containing the evidence term and stores it in the cache, so subsequent encounters of the same pair hit Tier 2.

## Translating canonical IR to the backend's language

The backend receives canonical IR. It must translate to its own input language. Three patterns:

### Pattern A: SMT-LIB emitter

For SMT-based backends (Z3, CVC5, Yices). The kit's `provekit-ir-compiler-smt-lib` emits SMT-LIB; the backend pipes it to the solver, parses the response.

The Rust kit's SMT-LIB emitter is generated from the CDDL grammar; an alternative-backend implementation that bridges to a different SMT solver can reuse the emitter directly.

### Pattern B: tactic-language emitter

For interactive theorem provers (Coq, Lean, Isabelle). The backend emits the formula as a goal, applies a fixed tactic strategy (e.g., `auto`, `omega`, `decide`), and captures the proof term.

This is more involved because tactic strategies can fail; the backend may need to try multiple tactics or accept partial-proof results.

### Pattern C: model-checker emitter

For bounded model checkers (CBMC, JBMC, NaSpec). The backend emits the formula as a verification condition, runs the checker, parses the result. Counterexamples come back with concrete variable values.

## Evidence-term serialization

The protocol stores evidence terms as opaque bytes inside an evidence envelope (similar shape to a claim envelope). The verifier checks:

1. Evidence envelope's signature is valid.
2. Evidence envelope's `prover_kind` matches a configured trusted prover.
3. Evidence envelope's content is well-formed for that prover's evidence shape.

The verifier does NOT re-run the prover. Trust is in the signature: "this prover, identified by this signing key, asserts that this `(post, pre)` pair was discharged by this evidence."

A user who doesn't trust the signing key can re-run the prover offline. The protocol gives them the evidence to do so.

## Configuration

A user configures which backends are trusted in `provekit.config.yaml`:

```yaml
trusted_provers:
  - "z3-foundation-ed25519"
  - "lean-foundation-ed25519"
  - "cbmc-foundation-ed25519"

reject_unknown_provers: true
```

The kit checks this configuration when verifying implication mementos. Mementos signed by untrusted provers fail the handshake; the verifier falls back to Tier 3 with a configured backend.

## Soundness considerations

A backend's soundness is the user's TCB, not the protocol's. The protocol is a vehicle for portable evidence; it does not endorse any backend's correctness.

This is a load-bearing claim of the protocol design. ProvekIt does not say "Z3 is sound." It says "if you trust Z3, here's a way to publish Z3's findings as portable, signed mementos." Users who reject Z3 reject the mementos signed by the Z3 backend. Users who accept Z3 accept those mementos.

A backend's documentation should make its soundness claims explicit:

- "This backend trusts Z3's `unsat` results."
- "This backend trusts the OCaml extracted code from Coq."
- "This backend bounds CBMC at depth 8; deeper bugs are not caught."

## Performance and the lattice

The backend is a hot path on first encounter of a `(post, pre)` pair. Subsequent encounters hit Tier 2 in the cache.

The lattice growing means that the backend's load decreases over time per project. As the cache fills up with discharged pairs, the residue at Tier 3 shrinks. Performance optimizations matter for the early days of a codebase; mature projects rarely hit Tier 3.

## Shipping checklist

- [ ] Subprocess interface implemented (stdin/stdout NDJSON).
- [ ] Canonical IR → backend input translation.
- [ ] Backend output → evidence-term serialization.
- [ ] Signing key configured (Ed25519, separate from foundation key, dedicated to this backend's identity).
- [ ] Documentation of soundness claims.
- [ ] Conformance tests against a fixture set of `(post, pre)` pairs with known-correct answers.
- [ ] Performance characterization (median proof time, memory ceiling).
- [ ] Integration tests with at least one shipping kit.

## When this is done

The backend is shipping. Users who configure it as trusted can verify their codebase against this backend's soundness claim. Implication mementos signed by this backend land in the lattice and are reusable by any verifier that also trusts the backend.

A new prover backend is a TCB choice for users; ProvekIt's protocol is the vehicle that transports its evidence.

## Read next

- [docs/reference/handshake/tier-3-solver-fallback.md](../reference/handshake/tier-3-solver-fallback.md) (when written) — Tier 3 reference.
- [docs/security/solver-trust.md](../security/solver-trust.md) (when written) — what trusting a backend buys.
- [docs/explanation/compared-to/coq-fstar-lean.md](../explanation/compared-to/coq-fstar-lean.md) (when written) — comparison to interactive theorem provers.
