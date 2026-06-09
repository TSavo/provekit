# ProvekIt Invariants

The load-bearing laws of the substrate. Violating one is the bug, not a judgment call.
These were re-derived many times before being written down; check work against them.

## I. What correctness is

1. **Correctness is four parts, relative to asserted claims.** A spec exists; the spec is
   coherent; a program satisfies it; a witness demonstrates it. Not "compiles," not "green
   tests," not coverage — those are everyone else's gates, assumed here as axioms.

2. **We see only claims.** A program enters our universe solely through its assertions. A
   claim is an operator over operands — a first-order-logic atom. A line that asserts nothing
   (a binding, setup, `return;`) is *invisible*: not a gap, not N/A, not in any denominator.
   No contract = nothing to prove.

## II. What we are not

3. **Not a compiler.** Compiler facts are axioms: if it compiled, types, borrows, drops,
   Send/Sync, lifetimes all hold — given, never re-checked. By construction we only ever lift
   from compiling code, so every contract is born type-valid; an ill-typed composition cannot
   arise upstream of us.

4. **Not a type system.** We never validate types — the compiler did. We carry canonical
   *identity* (to name subjects and keys), never type-validation. The moment a "fix" looks
   like type relationships or a type hierarchy, it is reinventing the compiler — stop.

5. **Not an effects theory.** We never model effects or assume determinism. Pure/concrete
   operands coalesce (same subject); impure/symbolic operands stay locus-distinct (their own
   subject). Both are sound; we simply decline to assume two impure calls are equal.

6. **Not a correctness checker.** The solver decides — z3, coq, vampire, maude. We lift to IR,
   form the obligation, and route it to a prover. We do not implement theories: floats are
   z3's FP theory, strings z3's string theory, quantifiers go to vampire/coq.

## III. The spec

7. **The vendor's tests are the spec.** The contract is lifted from the library's own tests.
   No declarations, no annotations (no JSR-380, no hand-added `#[requires]`), no authored
   contracts, zero changes to the code under proof. If the vendor never asserted it, we are
   silent on it — and that is correct, not a gap.

## IV. The layering

8. **Lifter -> ProofIR -> CLI -> solver, and language lives in exactly one place.** All
   language-specifics live in the per-language lifter; it emits language-agnostic ProofIR; the
   CLI / verifier / libsugar are language-blind (no rustiness, no per-language logic); the
   solver discharges. Test: the IR for a value is the canonical shape *any* lifter would emit,
   so it federates by CID.

## IVb. The lifter roles

Every lifter is per-language/per-platform, speaks RPC to the CLI, and emits language-agnostic
FOL ProofIR (invariant 8). Their roles are distinct and map onto the four parts of correctness:

- **Contract lifter** — reads the vendor's own tests/source and lifts the *claims* into contract
  rows (the `#euf#` atoms, pre/post). The spec originates here (invariant 7). Correctness #1
  (a spec exists). [`sugar-lift-rust-tests`, `sugar-lift-java-tests`, `sugar-lift-python-source`]
- **Implication lifter** — lifts the seam obligations: the producer's post conjoined as
  antecedent against the consumer's pre (`post |= pre`), at every call / `.await` / channel /
  lock boundary. Correctness #3 (the program satisfies the spec). [the handshake /
  `build_implication_obligation` / the effect edges]
- **Bridge lifter** — lifts a cross-language / FFI call edge into a `CallEdgeDecl`
  (`sourceContractCid` -> `targetSymbol`/`targetContractCid`), keying the caller's callsite to
  the callee's contract by symbol-CID so the conjoiner binds them across `.proof`s. Makes
  cross-language correctness work — the symbol-CID is the identity, no hub (invariant 10). [the
  Panama lifter, `cpython_ctypes_resolver`, the go cgo resolver]
- **Witness lifter/runner** — reruns the real tests, content-addresses the outcomes, and
  discharges only when the suite re-runs cleanly. Correctness #4 (a witness demonstrates it).
  [`java_junit_witness_rpc`, the pytest witness]

Coherence (correctness #2) is **not** a lifter — it is the verifier's z3-SAT over the conjoined
contract. Lifters produce claims/obligations/edges/witnesses; the solver decides (invariant 6).

## V. The logic

9. **ProofIR is first-order logic.** Atoms plus and/or/not/implies/forall/exists.
   Value-equality is just the simplest atom. The lift is a walk over the program structure
   (V, A, <=): bindings introduce operands (V, no claim), assertions are atoms (A, the claims),
   threaded in source order. Nested calls key recursively — the outermost call is the subject,
   inner calls are operands.

## Vb. The sort universe

9a. **The sort universe primitives are `Int`, `Real`, `Bool` — platform-free, abstract**
    (`sugar-ir-types::PrimitiveSortName`). Number values live here; a float *value* is a `Real`.
    Number itself has no platform intrinsics: no width, no wrapping, no endianness, no `usize`.

9b. **Platform intrinsics live in the kits, never as IR sorts.** Bit-widths (`i32`/`u8`/`usize`),
    wrapping/overflow, the `f32`/`f64` IEEE bit-representation, `size_of`/`align_of`, endianness
    — all resolved in the per-platform kit, as *refinements over `Int`/`Real`* with the platform
    semantics preserved as FOL constraints: `u8` -> `Int` with `0..=255`; `i32::wrapping_add` ->
    `Int` `(a+b) mod 2^32`; `usize`/`size_of` -> `Int` + a platform-width refinement; finite
    float -> `Real`; `f32`/`f64` IEEE semantics -> kit refinement over `Real`. Kits speak RPC to
    the CLI and emit FOL-only ProofIR; the CLI/verifier stay platform-blind (only the primitives
    + FOL); the solver discharges. The base sorts federate — any kit emits the same canonical
    `Int`/`Real` for the same value (same CID), platform semantics riding as preserved
    constraints. **Dropping a platform semantic is unsound** (treating `i32` as unbounded, a
    generic without its type arg): place it in the hierarchy *and* preserve the semantics, or it
    falsePasses on the platform.

    > Leak to evacuate: the legacy `Sort::Float { width }` carries a bit-width — a platform
    > intrinsic — inside an IR sort, and defers IEEE semantics (#385). Per this invariant that
    > width belongs in the kit as a refinement over `Real`, not as an IR sort. Float values
    > already lift to `Real`; `Float{width}` is the residue to evacuate (or the #385 stub).

## VI. Identity and federation

10. **The CID is the identity. There are no hubs.** Identical canonical shape -> identical CID,
    automatically, across languages and across time. Federation and cross-language binding are
    byte-identical CIDs plus the bridge memento (post |= pre at the call edge) — never a
    concept layer, a naming registry, or an identity hub.

10b. **Cross-FFI works because every lifter shares one canonical form — not a hub.** For a
    caller's bridge edge to bind a callee's contract row across languages, both lifters must
    produce *byte-identical* canonical output for the same logical content:
    - the `#euf#` subject key (callee + canonical arg-signature) — same call -> same key;
    - the canonical sort/value encoding (erase to the same `Int`/`Real`; `4` is `i:4` from any
      lifter; a float value is the same `Real` everywhere);
    - the canonical FOL/JCS bytes of the atom — same claim -> same bytes -> same CID;
    - the symbol-resolution convention (`<kit>:<symbol>`) — the bridge's `targetSymbol` names
      the callee exactly as the contract lifter keyed its row.
    This is enforced, not hoped: every kit must pass the cross-language conformance fixtures —
    *same canonical formula in, same bytes out*; a kit that diverges fails conformance. The
    shared canonicalization is what lets the CID *be* the identity across the FFI boundary —
    the bridge binds caller<->callee by that CID, with no concept layer between them.

10c. **The witness lifter is the runtime half of cross-FFI.** The static bind (10b) says the
    contract *crosses* the boundary; the witness says it actually *executes and behaves*. For a
    Java caller into a Rust callee, the witness lifter reruns the real cross-language execution
    — the Java program genuinely invoking the Rust function through the FFI — and
    content-addresses the outcome into a reproducible witness package, re-verified by CID +
    recompute, never a trusted run (invariant 12). Both halves are required: the shared
    canonical CID binds the contract statically; the content-addressed reproducible witness
    demonstrates the execution. A green run nobody can recompute is not a witness.

## VII. Soundness

11. **Sound by construction; conservative; one-directional.** We never falsePass. Identity is
    over-precise on purpose — carry the type argument, the concrete args, the locus — so
    distinct subjects never falsely coalesce. The only permitted failure mode is conservative
    over-refusal.

## VIII. The artifact

12. **The .proof is content-addressed and re-verifiable.** We do not ask for trust; we hand
    over a recomputable artifact. Everyone else asserts their code is correct; we `.proof` it.

## IX. How we work on it

13. **Lift changes are shared substrate: additive and backward-compatible.** Preserve existing
    `#euf#` keys; after any lift edit, confirm the sibling showcases still bind.

14. **Verify the mechanism, not the report.** Confirm the actual GitHub CI conclusion; read the
    runner for real `sugar` invocation; reject hardcoded verdicts and tautological self-checks.
    A local pass is not a CI pass.

15. **Never fabricate; report honest gaps.** A faked row, proof, or logo is worse than none.
    Honestly-scoped beats fake-full.

16. **Python is the reference — mirror, don't reinvent.** When a mirror is broken, diff it
    against the working reference; the bug is the diff. Do not generatively re-derive a
    mechanism that is already solved.
