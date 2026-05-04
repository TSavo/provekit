# Proof IR — and why not LLVM IR / SMT-LIB / Coq terms / Boogie / TLA+

## Why "Proof IR"

**Proof IR is literally the intermediate representation of proofs.** The name is descriptive, not aspirational. Same shape as the names of every other IR in computing:

| IR | What it represents |
|---|---|
| LLVM IR | the IR for LLVM (compilation) |
| MLIR | multi-level IR (compilation) |
| GIMPLE | the IR for GCC (compilation) |
| HIR / MIR | Rust's high / middle IR (compilation) |
| **Proof IR** | **the IR for proofs (verification + composition)** |

That's the answer. The reasoning that follows explains why none of those existing IRs covers what Proof IR covers — but the name itself is the same naming pattern those IRs all use, applied to a domain that didn't have one yet.

The competitor isn't another IR. The competitor is the absence of an IR for proofs. ProvekIt fills that gap.

## The real innovation: byte-ordering invariance and representation invariance

Existence of an IR for proofs is mundane. Boogie IL, Why-IR, and others have intermediate representations of proof obligations. The hard part — and the actual innovation — is that for any given proof, Proof IR has:

- **Byte-ordering invariance.** Same logical content produces bytes in the same order. Every machine. Every language. Every prover. JCS canonicalization (RFC 8785) for JSON content, locked CDDL key orders, deterministic CBOR encoding (RFC 8949 §4.2.1) for binary content. No floating ordering decisions, no implementation-defined behavior, no "we sorted alphabetically here but lexicographically there." One canonical order.

- **Representation invariance.** Same logical content produces ONE canonical encoding, not "an encoding among many." There is exactly one valid byte sequence for a given proof. Differently-encoded "equivalent" forms are not equivalent to the substrate; only the canonical form has standing.

These two invariants are what make content-addressing meaningful. `BLAKE3-512(proof_bytes)` identifies the proof ONLY if `proof_bytes` is canonical — otherwise the CID identifies *one of N possible encodings* of the proof, and the substrate's whole identity model collapses. With invariants, the CID *is* the proof's identity.

**Content-addressing lives above the language.** Once the IR is byte-deterministic, the content-CID is independent of which kit emitted it. A Rust kit and a Python kit producing proofs of the same logical claim produce byte-identical output. Their CIDs match. Their signatures over those bytes compose. The substrate's primitives (federation, multi-prover consensus, hash-preserving translation across languages) all stand on this property.

If the bytes weren't canonical, "content-addressable" would be marketing. The CID would identify "this particular encoding" rather than "this proof." Federation would fail at the first cross-kit composition: signer A's CID would not equal signer B's CID for the same logical proof. Multi-prover consensus would fail at the first divergent encoding. The substrate's cryptographic identity claim wouldn't hold.

This is what other IRs that exist for proofs (Boogie IL, Why-IR) don't have. They are intermediate representations of proof obligations within their respective frameworks. They aren't byte-canonical wire formats whose CIDs are stable across machines, languages, and provers. The existence-of-the-IR is the mundane part; the byte-ordering + representation invariance is the move.

**ProvekIt is therefore a universal protocol for correctness, not a tool for one ecosystem.** Any language, any prover, any artifact with logical structure can be brought into the substrate by giving it a lifter that emits canonical bytes. The protocol doesn't care about the source language; it cares about whether the bytes meet the canonicalization rules. Universality is a *consequence of the invariants*, not an add-on feature. A language that doesn't exist yet, a prover that hasn't been built yet, a verification framework someone will design next decade — all of them join the federation by emitting canonical Proof IR bytes. The substrate scales to languages and tools we haven't imagined, because the canonical-form rules don't depend on the source.

For the formal treatment of this property — same algorithm in different languages producing the same CID — see [Hash-Preserving Translation](../../papers/03-substrate-not-blockchain.md) (the manifesto's framing) and the bridge linkage protocol spec.

## Why none of the existing IRs work

The skeptic asking "why didn't you use X?" is implicitly assuming the IR's job is what X already does. The answer is that the IR's job is something X doesn't do. Different requirements, different design, different name.

Five properties Proof IR has, simultaneously:

1. **Federated** — multiple independent signers can attest claims using it; trust is composable across signers without a central authority
2. **Content-addressable** — every artifact is named by the BLAKE3-512 hash of its canonical bytes; identity is intrinsic, not registered
3. **Multi-prover** — same formula can be dispatched to a portfolio (default today: Z3, CVC5, Vampire, Coq); the protocol supports `first-wins` (default) and `consensus` modes per `.provekit/config.toml`
4. **Language-agnostic** — the IR is independent of the source language; lifters from rust / python / php / etc. all produce the same canonical bytes for the same logical content
5. **Signed-claim** — every contract is wrapped in a signed envelope with the signer's identity content-addressed

No existing IR has all five. Walking through them.

### vs. LLVM IR / MLIR / GIMPLE / HIR / MIR — compiler IRs

Compiler IRs are intermediate representations of *code* for compilation to *execution*. They preserve **operational semantics** — what the machine does, instruction by instruction. Loop unrolling, dead-code elimination, register allocation. The bytes describe a computation.

Proof IR is the intermediate representation of *claims* for verification across *provers*. It preserves **logical content** — what is asserted, predicate by predicate. Quantifiers, equalities, contracts. The bytes describe a logical structure.

Different domain. Using LLVM IR as the basis for Proof IR would be solving a different problem — running the code, not reasoning about it.

### vs. Coq terms / Lean expressions / Agda terms — proof-assistant terms

Each of these is locked to one prover's type theory. Coq terms only mean things in Coq (calculus of inductive constructions); Lean expressions only in Lean; Agda terms only in Agda. They have incompatible reduction rules, incompatible metatheoretic properties, incompatible elaboration semantics.

Proof IR is multi-prover by design. The same formula at the IR level is sent to Z3 (an SMT solver), Coq (a proof assistant), Vampire (a first-order resolution prover), CVC5 (another SMT solver) — and consensus across their verdicts is what binds the result. The IR is the lingua franca *between* provers, not a single prover's internal representation.

Using Coq terms as the canonical IR would lock the substrate to Coq and break the multi-solver consensus property the protocol depends on (per [paper 05](../../papers/05-witness-pluralism-and-jurisdiction-neutral-transport.md)).

### vs. SMT-LIB

SMT-LIB is the standard exchange format for SMT problems — first-order logic over decidable theories (linear arithmetic, bit-vectors, arrays, uninterpreted functions). It's well-supported by every SMT solver. Most automated verification today emits SMT-LIB.

Proof IR can't be SMT-LIB because SMT-LIB is first-order. It can't express higher-order reasoning (lambda over function-typed parameters), dependent types (`Vec(n)`, `forall n: nat`), or arbitrary computation. Those are exactly the cases the multi-solver portfolio's Coq seat covers — Coq translates them, SMT-LIB marks them opaque per the [opacity manifest spec](../../../protocol/specs/2026-05-02-opacity-manifest-grammar.md).

SMT-LIB is what Proof IR *compiles to* when an SMT solver is the target. It's one back-end format, not the source representation. Using SMT-LIB as the canonical IR would lose the higher-order / dependent-type expressiveness that makes the multi-solver portfolio sound.

### vs. TLA+ / Dafny / Verus IR — specification languages

These are author-facing languages designed for humans to write specifications. Rich syntax, expressive features, human-readable forms. TLA+ for distributed systems specifications, Dafny for verified imperative programs, Verus for verified Rust.

Proof IR is wire format — content-addressable canonical bytes lifted *from* source code by lifters, not written by hand. The shape is optimized for byte-determinism and verifier consumption, not human authoring. JCS canonicalization, locked key orders, content-CIDs.

Specification languages don't canonicalize for hashing, don't have content-CID models, don't byte-roundtrip. Different role. Using TLA+ as the canonical IR would break content-addressability, break federation, break the substrate's primitives.

### vs. Boogie IL / Why-IR — verification ILs

These are the closest neighbors. Boogie IL (Microsoft) and Why-IR (Why3) are verification-specific ILs that sit between specification languages and SMT solvers. They share Proof IR's role of "intermediate language for verification."

But they're tied to specific verification frameworks. Boogie IL is shaped by Microsoft's tooling (Spec#, Dafny, Viper); Why-IR is shaped by the Why3 platform. Neither is designed for federation across multiple language kits or multi-prover consensus. Neither is content-addressable canonical bytes by design.

Using Boogie IL would inherit Microsoft's tooling assumptions. Using Why-IR would inherit Why3's. The substrate would no longer be jurisdiction-neutral or framework-agnostic. The tools that depend on either would also gain implicit coupling. ProvekIt's federation property would be broken at the IR layer.

### vs. MSIL / CIL / JVM bytecode — runtime ILs

These are operational ILs for managed runtimes. They preserve execution semantics, not logical claims. Same category error as compiler IRs — they're about running code, not reasoning about it.

## The unifying answer

| Property | Compiler IRs | Proof-assistant terms | SMT-LIB | Spec langs | Verification ILs | Runtime ILs | **Proof IR** |
|---|---|---|---|---|---|---|---|
| Federated | — | — | — | — | — | — | **✓** |
| Content-addressable | — | — | — | — | — | — | **✓** |
| Multi-prover | — | — | (one tool family) | (varies) | (one framework) | — | **✓** |
| Language-agnostic | (varies) | — | (~partial) | — | (one source) | — | **✓** |
| Signed-claim | — | — | — | — | — | — | **✓** |

Each existing IR has at most three of the five properties. None has all five. The combination — federated AND content-addressable AND multi-prover AND language-agnostic AND signed-claim — is the design space the protocol needed and didn't exist.

That's why a new IR exists and a new name was minted. Same naming pattern as every other IR (LLVM IR, GIMPLE, MIR), applied to the gap.

## One-line version

> Existing IRs preserve execution, prover semantics, or framework conventions. None preserved logical content as content-addressable signed bytes federated across languages and provers. Proof IR fills that gap.

## See also

- [Whitepaper](../../papers/01-whitepaper.md) — what ProvekIt is and why
- [Bluepaper](../../papers/02-bluepaper.md) — formal protocol spec
- [Substrate, not Blockchain](../../papers/03-substrate-not-blockchain.md) — multi-dimensional pinning, the architectural foundation
- [Witness Pluralism](../../papers/05-witness-pluralism-and-jurisdiction-neutral-transport.md) — the substrate-independence theorem; multi-prover consensus
- [coq-fstar-lean.md](coq-fstar-lean.md) — ProvekIt-the-tool vs. proof assistants (different comparison)
- [kani-prusti-creusot.md](kani-prusti-creusot.md) — ProvekIt-the-tool vs. Rust-specific verifiers (different comparison)
