# ProvekIt Invariants

The load-bearing laws of the substrate. Violating one is the bug, not a judgment call.
These were re-derived many times before being written down; check work against them.

## 0. The one unique thing (the product)

We make exactly one thing: **#1 — the spec: the ProofIR, lifted from code.** It is
content-addressed, federating first-order logic. That is the invention and the moat. Everything
else in this document — the lifters, the sort universe, erasure/recovery, vendor-tests-as-spec,
the canonical CID, no hubs — exists only to make that spec *right*.

The rest of the tuple is **composition with tools that already exist**: **#2** is the solver
saying the spec is coherent (z3-SAT over the IR) — and **#1 and #2 are one object, a coherent
spec** (the lifted spec plus the proof it is consistent); **#3** is the **program** itself (the
input we prove); **#4** is the **witness** (re-run + recompute the CID). The *satisfaction*
(the program satisfies the coherent spec) is what we prove — a solver discharging `post |= pre`
(z3 / coq / vampire) — not a thing we invent. Hand an off-the-shelf prover or runner the IR and
it does coherence, satisfaction, and the witness check. **The product is #1, the spec/ProofIR;
the rest is plumbing it into machines that already exist.** This is why we are not a compiler,
type system, effects theory, or checker (II): the only thing we build is #1.

**One sentence, written N times.** What follows is not N independent laws. It is a single
sentence in N costumes: *the canonical structure is the only real thing, its content address is
its name, and everything else is a shadow.* From clause one (structure is the only real thing):
we see only claims, and behavior, types, and every effect are invisible (II, V, and #2 below).
From clause two (its content address is its name): the CID is identity, composition is lawful,
and federation needs no hub (VI). From clause three (everything else is a shadow): we hand over
recomputable structure, never a version number, a coverage percent, or a squiggle (VIII). That a
pile of rules collapses to one sentence is the test that this is a theory and not a checklist:
read it from any of the N angles and the same thing comes back.

**Every language is Sugar.** Not just operators (9x), not just function names (VI) — the entire
source language is surface syntax that desugars to ProofIR *at the boundary*, because **ProofIR
is the language of boundaries**. Claims exist only at seams — an assertion, a call edge, an
`.await`, an FFI crossing, a version bump — and the only thing that survives crossing a boundary
is the contract, whose language is ProofIR. That is *why* effects are invisible (they never
cross a boundary as a claim, so they were never in the language we speak); *why* the lifter is a
desugarer, one per language, and the core is language-blind (IV); *why* federation needs no hub
(two languages meeting at a boundary both desugar into the boundary-language, so identical
claims get identical CIDs by construction); and *why* `sugar diff` works (a release is a
boundary too). The product is named for this theorem.

## I. What correctness is

1. **Correctness is a four-slot tuple, relative to asserted claims.** #1 the **spec** (the
   ProofIR, lifted from code); #2 the **solver saying the spec is coherent** — and #1+#2 are one
   object, *a coherent spec*; #3 the **program**; #4 the **witness**. Correctness is the proof
   that the program (#3) satisfies the coherent spec (#1+#2), demonstrated by the witness (#4).
   Satisfaction is what we *prove* about #3, not a slot. Not "compiles," not "green tests," not
   coverage — those are everyone else's gates, assumed here as axioms.

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

   Failure semantics are invisible by construction. A body guard's condition lifts as a flat
   predicate (`if x == 0 { panic!() }` becomes the precondition `x != 0`, a claim); the panic
   itself asserts nothing to us, exactly like a no-op line (invariant 2). Effects are never
   *loudly bounded lossy* either: that band of the trichotomy (exact / loudly-bounded-lossy /
   refuse) only exists when we hold a representation and announce what we dropped from it (an f32
   kept as `Real`, IEEE width logged as a named residual). We hold no representation of a panic,
   so there is nothing to bound the loss on. The only honest moves are lift the precondition
   exactly, or REFUSE (skip and name the residual). There is no lossy model of a failure; if
   there were a failure-model to approximate, we already crossed into effects and went wrong.

   The same holds for async and concurrency. We never model what `.await`, a channel, or a mutex
   *does* (suspend, poll, schedule, resume); that is an effect. So `.await`, a channel send, a
   lock acquire, a `throw`, an FFI call, and a plain synchronous call are ONE primitive to us: a
   seam (a locus, its own subject) carrying a `post |= pre` obligation. We do not keep four
   effect theories for four kinds of boundary; we keep one boundary with one obligation, and the
   *kind* of boundary (the async-ness, the FFI-ness, the lockedness) is invisible. Refusing to
   model the effect is exactly what collapses those into a single seam primitive, and it is why
   we cover async and concurrency without ever owning an effects theory. Failure was only the
   first effect, not the only one.

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
- **Witness lifter (kit oracle)** — WITNESSES THE PROGRAM: it resolves the witness body — the
  run that *demonstrates the program satisfies its spec* — over `sugar.plugin.resolve_witness`
  (from the package, or by re-running). This is correctness #4, and it is what **completes the
  correctness tuple** (spec exists, coherent, satisfied, *witnessed*) for *any* program. It has
  nothing to do with cross-language — every proof's tuple is completed by its witness. The kit
  is UNTRUSTED and only hands back bytes. [`java_junit_witness_rpc`, the pytest witness]

Verification of the witness lives in the **rust CLI** (`witness_verify`), never the kit: rust
checks the ed25519 signature over the witness CID with its *own* primitive, and **recomputes**
the blake3 of the resolved bytes against the pinned `witness_cid`. A body that does not
recompute is a *broken oracle*, refused loudly. **Trust the recomputation, never the resolver.**
That is correctness #4 made trustless — the sharpest form of invariant 12.

What the witness body literally *is*: the mundane attestation the world already calls proof — a
JUnit PASS log, a vitest coverage report, a signature on a page. On its face it is a squiggle:
trivially faked, worth nothing. It becomes worth something only because rust recomputes its CID.
Everyone else ships the squiggle and says *trust me* (the green badge, the coverage percent, the
signature); we ship the squiggle **plus the recomputation that makes faking it pointless**.

Coherence (correctness #2) is **not** a lifter either — it is the verifier's z3-SAT over the
conjoined contract. Lifters produce claims / obligations / edges / witness-bytes; the rust CLI
verifies (recompute, signature, solver) — invariant 6. The kit proposes; rust disposes.

## V. The logic

9. **ProofIR is first-order logic.** Atoms plus and/or/not/implies/forall/exists.
   Value-equality is just the simplest atom. The lift is a walk over the program structure
   (V, A, <=): bindings introduce operands (V, no claim), assertions are atoms (A, the claims),
   threaded in source order. Nested calls key recursively — the outermost call is the subject,
   inner calls are operands.

9x. **`==` is sugar.** A source-language equality or comparison operator is sugar for a method
    call — `PartialEq::eq`, `.equals()`, `__eq__` — exactly as function names are sugar (VI).
    FOL `=` / `distinct` / `<` are reserved for PRIMITIVE terms (literals, known-scalar call
    results). An operator whose operand is a non-primitive (constructor / user-typed) term
    lifts as the *operator-dispatch call atom* — an uninterpreted per-type call-result row
    (`=(call:eq:<TypeKey>(a,b), true|false)`), never FOL equality: the user impl can make `eq`
    anything (std's own `cmp_default` test ships a deliberately inverted `PartialEq`), and FOL
    `distinct(x,x)` is unsatisfiable while `x != x` can be *true in the language*. EUF keeps
    this sound and point-wise faithful (congruence forces same-args -> same-result only). This
    shape is the FEDERATED canonical atom (10b): every seat's `==`-on-objects desugars to it
    byte-identically. The distinction is syntactic (is the operand a primitive term?), not type
    inference — we are still not a type system (II). Lifting FOL `=` over a user-typed term is
    the overclaim that false-refused std's `cmp_default`; the gate question for every lifter's
    `=` is "what does this operator dispatch to?"

    And the constructive half: **a user's `equals` is a function with a contract, and we lift
    it.** The dispatch atom is not a refusal — it is the vendor's equals contract as ordinary
    `#euf#` call-result rows (`cmp_default`'s inverted `eq` becomes four point-wise claims,
    proven consistent). Equality thereby inherits the whole substrate for free: the rows
    coalesce and conjoin (a consumer contradicting the vendor's `eq` is a real z3 refusal),
    federate by CID, carry seam obligations (a pre that relies on equals behavior), and move
    under `sugar diff` when a release changes what `equals` does. No new machinery — the
    operator was a method call all along, so the method-call substrate simply applies.

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

    > Evacuated (main `4b45d9f48`): the legacy `Sort::Float { width }` carried a bit-width — a
    > platform intrinsic — inside an IR sort. It is now cut from the IR entirely. Float values
    > lift to `Real`; finite float constants emit `Real` decimal strings; `f32`/`f64` IEEE
    > semantics (NaN/inf/orderedness/±0/width) are named kit-refinement residuals over `Real`,
    > not IR sort identity. The IR grammar CID changed accordingly; no committed `.proof`
    > carried the old sort, so federation is unbroken.

## VI. Identity and federation

10. **The CID is the identity. There are no hubs.** Identical canonical shape -> identical CID,
    automatically, across languages and across time. Federation and cross-language binding are
    byte-identical CIDs plus the bridge memento (post |= pre at the call edge) — never a
    concept layer, a naming registry, or an identity hub.

    Composition is lawful *by construction*, not by luck. Because the CID is a deterministic
    function of canonical structure, regrouping cannot change it (associativity), and the no-op
    seam leaves it fixed (identity). Those are the monad laws, and they are forced, not observed.
    That lawfulness *is* federation: the same logical composition yields the same CID no matter
    who assembled it or in what order, which is exactly why no hub is needed to reconcile two
    parties. Remove the laws and federation collapses into needing a hub. "Composition is
    content-addressed and canonical" and "composition satisfies the monad laws" are one statement
    written twice.

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

## VII. Soundness

11. **Sound by construction; conservative; one-directional.** We never falsePass. Identity is
    over-precise on purpose — carry the type argument, the concrete args, the locus — so
    distinct subjects never falsely coalesce. The only permitted failure mode is conservative
    over-refusal.

## VIII. The artifact

12. **The .proof is content-addressed and re-verifiable.** We do not ask for trust; we hand
    over a recomputable artifact. Everyone else asserts their code is correct; we `.proof` it.
    The genre we replace is the shadow in a lab coat. SemVer projects an arbitrary behavioral
    change onto three integers and a publisher's promise (`^1.2.0` is a bet, enforced by
    nothing); the green badge attests one run; coverage counts lines, not claims; a type
    signature swears it typechecks, never that it upholds its precondition. Each computes a proxy
    for correctness and trains you to defer to the proxy. We do not ship a smaller proxy: we ship
    the thing the proxy stood in for, recomputable. SemVer says "minor, trust me"; we discharge
    `post |= pre` at the seam and hand over the math.

12b. **The whole `.proof` DAG closes to a `memcmp(64)`, and we sit at its head and its root.**
    Every `.proof` is a Merkle node: its CID is the blake3-512 (64 bytes) of canonical content
    that *includes the CIDs of its children* — a composition names its components by CID, a
    witness names its resolved bytes by CID, `k(I)=t` pins all three. So verifying an arbitrarily
    deep tower — a proof of a proof of a lifted contract over a witnessed build over a
    content-addressed source tree — reduces to recomputing the root and comparing 64 bytes.
    Collision resistance makes that single comparison transitively certify every node beneath it:
    you never walk the DAG, you recompute its head and `memcmp` 512 bits. **The total trust
    surface of the entire substrate is 64 bytes wide.** Not a signature (which trusts a key), not
    a verdict (which trusts an oracle), not a quorum (which trusts a majority) — byte equality,
    the one primitive that needs no trust because anyone can perform it and there is exactly one
    answer. We chose 512 bits, not 256, so that one comparison is unimpeachable enough to carry
    an unbounded DAG.

    This is *why* we are not a vendor (the no-vendor axiom): **you cannot solve a supply-chain
    attack by adding another vendor to the supply chain**, because trust does not compose, it
    accumulates attack surface — the xz maintainer *was* trusted; trust was the surface. We add no
    trust, only a recomputation, all the way down, including over our own witness (IVb). z3 and
    blake3 are not parties you trust; they are recomputations anyone re-runs. The day our own
    witness is trusted rather than recomputed is the day we *become* the attack — self-application
    is the whole proof.

    We sit at both ends, and the ends are the same kind of thing. We mint the leaves (the
    **root**: the lifters author the canonical bytes, so we decide what the CIDs *are*) and we
    perform the closing comparison (the **head**: the CLI recomputes and `memcmp`s, IVb). Because
    the output is itself a CID, the head of one chain is the root of the next composition (10) —
    head and root are not two ends of a line but a fixed point, the thing we produce and the thing
    we check meeting at `memcmp(64)`. Every soundness hole is exactly a place where some chain
    tried to close to something *other* than that comparison: a trusted discharge verdict instead
    of a recomputed CID, a merge waved through on "looks clean" instead of recomputed bytes. The
    entire job is keeping every chain closing to the 64 bytes, and refusing every shortcut that
    would close it to a trust instead.

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
