# ProvekIt compared to Coq, F\*, Lean (interactive theorem provers)

This is the comparison decision-makers most often need. The short version:

**ProvekIt is not in the same category. Coq, F\*, Lean, Isabelle, Agda are formal verification frameworks. ProvekIt is a protocol for content-addressing the verifications those frameworks produce.**

If you're choosing between "use ProvekIt" and "use Coq," you've miscategorized. The right comparison is "what's my verification framework?" (Coq vs. F\* vs. Lean vs. Isabelle vs. Z3 vs. Kani) and separately "how do I publish, distribute, and compose my framework's outputs?" (an ad-hoc registry vs. ProvekIt vs. nothing).

This doc unpacks why.

## What interactive theorem provers do

Coq, F\*, Lean, Isabelle, Agda are all **interactive theorem provers** (ITPs). They:

- Provide a logic (Coq's CIC, Lean's MLTT, F\*'s effectful HOL, Isabelle's HOL/ZF/etc.).
- Provide a proof language (Coq's Gallina + Ltac, Lean's tactic mode, F\*'s combinator-style tactics).
- Have a small, well-audited kernel that checks proof terms.
- Support extraction (Coq → OCaml, Lean → C, F\* → OCaml/F#, etc.).

The kernel is the TCB. A proof term checked by the kernel is sound modulo the kernel's correctness; the kernel is typically 10kloc of carefully-written code.

This is the highest assurance level available in formal verification. seL4, CompCert, Verdi, Tezos's smart contract verifier — all rely on ITP-level assurance.

## What ProvekIt does

ProvekIt does not produce verifications. ProvekIt provides:

- **Canonical IR**: a content-addressed representation for behavioral contracts.
- **Signed mementos**: contracts and implications, signed by the prover.
- **A handshake**: how a verifier discharges call sites against a memento pool.
- **A protocol**: how all this composes across the dependency graph.

ProvekIt invokes Z3 by default at Tier 3. Z3 is not an ITP; Z3 is an SMT solver, returning `unsat` heuristically without producing a kernel-checkable proof term.

So ProvekIt's default backend has lower assurance than an ITP. But:

- ProvekIt supports configuring different backends.
- ProvekIt supports requiring multi-backend concurrence.
- An ITP-backed ProvekIt setup has the same TCB depth as the ITP itself, plus the protocol's substrate.

## When you actually want an ITP

Use Coq, F\*, Lean, Isabelle, or Agda when:

- **The cost of a wrong claim is catastrophic.** Cryptographic primitives, OS kernels, life-safety code, financial settlement code, voting systems.
- **You can afford the cost.** ITP proofs are slow to write (months per kloc), require expert authors, demand continuous maintenance.
- **You need extracted certified code.** Coq → OCaml, Lean → C, etc. The extraction is verified.
- **Regulators or auditors require kernel-checked proofs.** Some certifications (Common Criteria EAL5+, DO-178C Level A) reference formal methods at this assurance level.

The use case is narrow. Most software does not need it. The cost is high.

## When ProvekIt complements an ITP

Use ProvekIt alongside Coq/F\*/Lean when:

- **You want to publish ITP-verified contracts in a portable, content-addressed form.** The ITP produces the verification; ProvekIt distributes it.
- **You have an ITP-verified library and consumers in multiple languages.** A bridge from a Coq-verified C function to a Rust consumer's contract becomes possible if the Coq verification is captured as a ProvekIt evidence term.
- **You want cross-domain verification without re-running the ITP.** Once Coq has discharged a `(post, pre)` pair and the result is signed and minted, every future verifier hits the cache.

The ITP is the verification framework. ProvekIt is the substrate over which the ITP's outputs become useful at scale.

## Worked example: Coq-verified parseInt

Suppose you have a Coq-verified implementation of parseInt:

```coq
Theorem parseInt_correct :
  forall (s : string),
    forall (n : nat),
      parseInt s = Some n ->
      string_of_nat n ⊆ s.
Proof.
  ...long proof...
Qed.
```

The proof term is checked by Coq's kernel. You're confident in `parseInt`'s correctness.

Without ProvekIt: `parseInt`'s correctness is a fact in your local Coq workspace. To make it useful to others:

- They run Coq.
- They check your proof.
- They trust the OCaml code Coq extracts.

This is workable but doesn't compose across languages. A Rust consumer of a different `parseInt` cannot benefit from your Coq verification.

With ProvekIt:

1. Your Coq theorem produces a proof term.
2. You publish a contract memento for the canonical `parseInt` reference contract (`ref-parseInt-v1`).
3. You publish an implication memento: "Coq theorem `parseInt_correct` (CID X) implies `ref-parseInt-v1` (CID Y), with this Coq proof term as evidence." Signed.
4. You publish a bridge: "OCaml `parseInt` extracted from Coq is bound to contract CID X."

A consumer in Rust, using a different `parseInt`:

1. Their Rust adapter lifts `parseInt`'s annotations to a contract.
2. They publish a bridge: "Rust `parseInt` is bound to `ref-parseInt-v1` (CID Y)."
3. The handshake walks: Rust contract → Y ← X ← Coq theorem.

The Coq verification's correctness now applies to a Rust consumer of a different `parseInt`. The transfer is hash-bounded.

This is what "composing across the dependency graph" means in the ITP world. Without ProvekIt, the Coq proof stays local. With ProvekIt, the Coq proof is portable.

## TCB comparison

| Approach | TCB |
|---|---|
| Coq alone | Coq kernel (~10kloc OCaml) + extraction (~varies) |
| Lean alone | Lean kernel (~12kloc C++) + extraction |
| F\* alone | F\* SMT-encoder + Z3 |
| Z3 alone | Z3 binary (~250kloc) |
| ProvekIt + Z3 | Z3 + protocol primitives (BLAKE3-512, Ed25519, JCS) + kits |
| ProvekIt + Coq backend | Coq kernel + protocol primitives + kits |
| ProvekIt + multi-backend concurrence | Multiple solvers + protocol primitives + kits (lowest TCB unless all backends are wrong simultaneously) |

The protocol primitives (BLAKE3-512, Ed25519, JCS canonicalization) are roughly comparable in audit difficulty to a kernel: thousands of lines, well-specified, widely-deployed.

The kits' canonicalizers add to the TCB: kit-specific code that must produce byte-identical output. Conformance fixtures defend against drift; the harness is the audit hook.

So ProvekIt with multi-backend concurrence is plausibly comparable in assurance to a single ITP, with the protocol layer as additional TCB. Trades for portability and amortized cost.

## What you lose vs. an ITP

ProvekIt does not provide:

- A kernel-checked proof of soundness for the protocol itself.
- A constructive proof for every discharge (only for ITP-backed discharges).
- A verified extraction story.
- A regulator-accepted assurance level (none of the major standards reference ProvekIt yet).

If your deployment requires any of these, an ITP alone (without ProvekIt) is the right fit. ProvekIt's value-add is when you also need cross-language portability, dependency-graph-scale composition, and amortized solver cost.

## What you lose vs. ProvekIt

If you go pure-ITP without ProvekIt:

- No content-addressed substrate. Sharing proofs across teams requires ad-hoc tooling.
- No bridges between proofs in different languages. Cross-language proof transfer requires custom encoding per pair.
- No cache effects. Every consumer re-runs the ITP (or trusts an ad-hoc artifact).
- No supply-chain anchor (no `binaryCid` equivalent in standard ITP workflows).

ProvekIt fills these gaps for the cases where ITP-level rigor is overkill but content-addressed verification is valuable.

## Choosing your stack

Decision tree:

1. **Does my deployment require regulator-accepted formal methods (Common Criteria, DO-178C, ISO 26262)?** → ITP. ProvekIt does not yet have regulator acceptance.
2. **Is my code tiny but high-stakes (cryptographic primitives, kernel modules, memory allocators)?** → ITP. The cost is justified.
3. **Is my code large, polyglot, and behavioral-contract-shaped (input validation, type checking, schema validation)?** → ProvekIt. The lift-not-author posture matters.
4. **Both?** → ITP for the high-stakes core, ProvekIt for the polyglot perimeter, with bridges connecting them.

The "both" path is rare today because ITP integrations with ProvekIt are nascent. Coq → ProvekIt evidence-term emission is in flight; Lean and F\* are TBD. As these integrations mature, "both" becomes practical.

## What ITPs would add

If a ProvekIt-Coq integration ships:

- Coq theorems can directly become ProvekIt mementos.
- Coq proof terms can be evidence in ProvekIt implication mementos.
- Verified OCaml extracted from Coq can be bound to ProvekIt contracts via bridges.

Same shape for Lean, F\*. Each integration is a contributor project; see [`../../contributing/writing-a-prover-backend.md`](../../contributing/writing-a-prover-backend.md).

The current state is "Z3 is the default backend; ITP backends are explicitly in scope but not shipping." If ITP integrations are critical for your deployment, this is the contribution that unblocks you.

## Read next

- [kani-prusti-creusot.md](kani-prusti-creusot.md) — Rust-specific provers.
- [`../../contributing/writing-a-prover-backend.md`](../../contributing/writing-a-prover-backend.md) — adding a new backend.
- [`../../security/solver-trust.md`](../../security/solver-trust.md) — TCB for different backends.
- [`../boundaries.md`](../boundaries.md) — what ProvekIt is NOT.
