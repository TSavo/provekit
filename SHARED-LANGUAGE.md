# ProvekIt — Shared Language

> The dictionary. T defines each term, plainly, once. Claude records exactly what
> was said — no reverse-engineered models, no additions, no running ahead.

## Lift

Lift has **two parts**:
1. It lifts **contracts** into **ProofIR**.
2. It lifts **sugar** into **body_text**.

## Lower — the mirror of Lift

Lower is the inverse of lift, and it has the **same two parts**:
1. It lowers a **contract** into a native artifact — a test, a gate, an annotation.
   (This is what we used to call **emit**.)
2. It lowers **sugar** into native source at a boundary — a library call.
   (This is what we used to call **materialize**.)

So the translation surface is a **2×2, not three verbs**:

|                       | contract | sugar |
|-----------------------|----------|-------|
| **in**  (native → IR) | lift     | lift  |
| **out** (IR → native) | lower    | lower |

`emit` and `materialize` were never two concepts — they are the **contract-facet** and the
**sugar-facet** of `lower`, exactly as contract-lifting and sugar-lifting are the two facets
of `lift`. `migrate`/`transport` were a *third* egress name for the same act ("migration is
materialize wearing a lab coat") — `lift` then `lower` into another library. The verb is
`lower`; the rest was sprawl.

Naming: **lift ⇄ lower** is the canonical compiler dual (raise to IR / descend to target),
a symmetric pair in both grammar and meaning — unlike "realize," which names an outcome, not
a direction. **"Realize" is reserved for the kit operation that *performs* a lower over RPC:**
lower is what you ask for; realize is the kit doing it for one language.

## Boundary

A boundary is a **realization site of a sugar**. It's a **client of a library**.
- The **library author supplies the sugar**.
- The **user of the library has boundaries** at which the sugar is materialized.

## Concept

A concept is a **name for a shared tag amongst sugars**.
Example: `json_encode`. Or `json_decode`.

## Contract

A contract is a **ProofIR first-order logic**. It's **tied to a specific sugar**.
- When you materialize the sugar, the **contract propagates with the sugar** to the
  materialize site **at the boundary**.
- So: **library authors declare "sugar x gets contract y."**
- Users get **red squigglies when they violate a contract at compile time.**

**Why it's tied to the sugar (the contractual reading).** A contract is *contractual*:
a **binding, dischargeable obligation**. `verify` **discharges** it (satisfies the
obligation); the solver is the discharge engine. Sugar is *that which is under contract* —
the subject the obligation is about. So sugar and contract are not independent payloads;
they are a **bound pair**: sugar is the subject, the contract is the encumbrance it carries.
That is **why** they co-travel — `lift` produces both, `lower` carries both, the contract
propagates with the sugar to the boundary. It isn't a mechanism choice: **sugar is never
naked.** A sugar without its contract is just a call; the contract is the lien that makes it
accountable, and you can't take the subject free of the obligation. A **boundary is where a
contracted sugar comes due** — which is exactly where the squigglies fire.

This also sharpens [concept equivalence](#concept-equivalence--vendors-call): a concept
clusters sugars; the federation question is just *"are these sugars under the **same**
contract?"* — bearing the same obligation. The vendor rules whether two encumbrances are
one encumbrance.

## Sugar

Sugar is **that which is under contract** — the arbitrary subject an obligation rides on.
The asymmetry with the contract is the load-bearing fact of the whole substrate:

- **ProofIR (the contract) is uniform.** *Always* first-order logic. One form in every
  domain — finite, dischargeable, **federatable across domains**, composing in the same
  solver whether it governs crypto or a tax rule. The logic never varies.
- **Sugar is unconstrained.** It is *literally anything*: a function, a poem, a Wikipedia
  article, an entire Rust codebase. Content-addressed but **uninterpreted** — the carrier
  has no required shape because the world has no required shape.

What the separation buys: **you can place a dischargeable obligation on something you can
never formalize.** ProvekIt models *nothing* about the subject — the sugar stays itself,
opaque — and the only formal object is the obligation on it. **Law over subject.** Federation
across domains then falls out for free: because every contract is FOL, the sugar's wildness
never touches the logic; the domain lives entirely in the sugar and the sugar never enters
the solver. The logic layer is **domain-blind by construction.**

This is the deepest reason there is **no bespoke contract language** (see
[`project_provekit_no_bespoke_contract_language`]): you were never meant to formalize the
subject. `.invariant` tried to author the sugar into a formal intermediate. The truth is the
inverse — leave the subject arbitrary, lift only the obligation, and the one formal
vocabulary (first-order logic) is **universal**: the same for the poem and the kernel. No new
language is possible or needed. **The substrate formalizes nothing about the world and can
still prove things about all of it.**

## Implication — the composition operator of the contract algebra

A contract is a pre/post over a sugar. Composing two operations — `A(B())`, B produces, A
consumes — is licensed by **exactly one** proof obligation:

```
post(B) → pre(A)
```

The producer's postcondition implies the consumer's precondition. **That arrow is an
implication** — the trinity's third element, the thing `cmd_implicate` mints. (`a | b` and
`A(b())` collapse to the same invariant: *post of whoever runs first → pre of whoever runs
next*; direction follows the data, not the parentheses.)

This is **Hoare's rule of composition**, content-addressed:

```
{P} B {Q}     {Q} A {R}
───────────────────────
     {P} B;A {R}
```

The meeting condition `Q` — where B's post must discharge A's pre — *is* the implication.
Sequential composition is sound iff that edge holds.

So the **trinity {terms, contracts, implications} is a graph:**
- **terms** = the operations — the **nodes**.
- **contracts** = pre/post obligations on each — the **node labels**.
- **implications** = `post → pre` — the **edges** that compose them.

Implications are the **durable** layer: a proven `P → Q` is a reusable edge — a lemma. Terms
and contracts are local; the edges between them are the composable, federatable,
content-addressed proof. Mint an implication once, compose with it forever.

## Composition & contradiction — why the AST-composition machinery is the spine

The three facts are one coin:
- **Composition over AST trees** = laying the `post → pre` edges along the call structure.
  You compose over the tree because the call graph *is* the composition order — it tells you
  whose post meets whose pre. (`libprovekit::compose` + the AST composition in
  `core/source_transform.rs` / `core/bind.rs`.)
- **Contradiction = a failed edge.** `post(B) ∧ ¬pre(A)` is SAT ⟹ the composition is unsound
  ⟹ **refuse**. Detecting a contradiction *is* finding a composition implication that does
  not hold. No composition → no conjunction for the solver → no contradiction → the **refuse**
  arm of the trichotomy has nothing to fire on. **Composition is upstream of *supra omnia,
  rectum*** — cut it and you remove the substrate's ability to say no.
- **A whole program verifies by discharging every edge,** grounded in `true`: the base
  precondition is ⊤ (the empty conjunction, `EMPTY_SET_CID`); every call adds one `post → pre`
  edge up the graph to the top-level postcondition. The proofchain `k(I)=t` is a DAG of
  implications rooted at ⊤.

Plainly: **a bug is a missing edge; a contradiction is a present-but-false edge; correct
software is a graph of contracts whose every composition edge discharges, rooted in `true`.**
This is why the promotion/consensus apparatus is severable cruft but the composition machinery
is not: composition is how contracts compose, which is how contradiction is solved.

## Where composition lives — per-language extraction, language-agnostic discharge

The rust CLI must stay **language-agnostic** yet compose **every** language's call graph. The
resolution splits call-edge *extraction* (per-lifter, language-specific) from call-edge
*composition + discharge* (rust CLI, language-blind).

**Every lifter must express, in universal ProofIR — and all 10 kits do today** (audited
2026-05-27: java, rust, python, go, cpp, csharp, ruby, swift, zig, c each emit both the
pre/post interface and call-edges; none is a bare-proposition emitter)**:**
1. The contract as **pre/post over the operation's symbol** — the typed interface, so the
   substrate can align one op's output to the next op's input. *Java today:*
   `ContractDecl { symbol, preconditions, postconditions, invariants }` →
   `{"precondition": …, "postcondition": …}` (`provekit-lift-java-core/ContractDecl.java`).
2. The **call-edges of its own language's call graph** — because only the java lifter can parse
   java, only the python lifter python. *Java today:* `ProductionWalk` finds each callee's
   callsites in each caller and, per hit, `substituteVar(callee.precondition,
   callee.formals[i], actualArg[i])` — i.e. `post → pre` **with variables aligned to the actual
   call** — emitting the implication (`provekit-lift-java-core/ProductionWalk.java`,
   `provekit-lift-java-junit`).

Call-edge **extraction is necessarily per-lifter** (it requires parsing the source language's
AST) and **all lifters must do it.** A lifter that emits a bare proposition with no interface
and no edges has produced a contract that cannot compose — composability is the whole game.
(The 2026-05-27 audit is a **presence** check — the interface + call-edge machinery exists in
all 10 kits. Byte-identical cross-language composition round-trip is a separate conformance
run, not asserted here.)

**The rust CLI stays language-agnostic** because all of the above crosses the RPC line as
**uniform ProofIR** — contracts + `post → pre` implications. The CLI composes and discharges
the graph (`provekit-linker`: `bindings = f(contracts ∪ call-edges)`; `libprovekit::compose`;
the solver) without ever knowing the source language. It "deals with all languages' call
edges" precisely because, by the time they reach it, they are no longer java/python/rust ASTs
but the one content-addressed implication form. **Language-specific parsing stops at the
lifter; everything above the RPC line is the universal graph of edges.** This is the founding
rule cashed out: *computation over data belongs in rust, post-RPC* — the lifter extracts
(language-bound), the CLI computes (language-blind).

## Kit

A kit is a **language-specific implementation of these ideas**. The **Java kit**:
- lifts **sugar bodies**
- lifts **contracts from JUnit tests**
- emits **witnesses as JUnit tests from contracts**
- writes **sugars into boundary bodies**
- handles **side-effect propagation** like import statements

## Concept equivalence — vendor's call

Whether several sugars sharing a concept mean the *same* FOL is **entirely the vendor's
decision. ProvekIt sets nothing.** (Mechanism: three-axis pinning — below.)

## Emitter — the contract-facet of lower

Turns ProofIR (a contract) into a concrete native artifact — a JUnit test, a Spring
annotation, a gate that throws, etc. A kit hosts **many** emitters; **which to emit is a
kit-time decision and you can invoke many** (one contract → stub + test + annotation +
gate, all at once). Inverse of the lift-from-native-test path: the emitter writes the
contract OUT as a test/annotation; lift reads it back IN.
(In the 2×2 above this is **lower(contract)**; **lower(sugar)** is the materializer.)

## The lift/lower asymmetry (load-bearing)

- **Lower is plural** (a relation): one contract/sugar → N faithful native forms,
  simultaneously. Safe, because every form is a *projection* of the one truth; none can
  contradict it.
- **Lift is singular** (a function): one surface → exactly one contract. Forced by
  content-addressing — if a surface could lift two ways, the ingested truth is ambiguous,
  the CID is unstable, pinning is meaningless, federation collapses. Two parties lifting
  the same surface MUST get the same contract.
- Plainly: **truth has one source but many expressions.** Lifting establishes truth (must
  be a function); lowering expresses truth (may be a relation).
- Refinement: a kit may have *many lifters* (one per distinct surface — tests, proptest,
  bean-validation, JML, …); the rule is per-surface determinism (one surface → one
  contract). Multiple surfaces on the same code each lift once and **compose/conjoin**.
  Forbidden thing = one surface → two contracts. (This is why `.invariant` died: a second
  lifting path competing with the canonical one.)

## Solvers — DERIVED FROM T ("verifies those contracts against solvers"), confirm

The discharge engine for contracts. A contract is ProofIR FOL; the solver composes,
refutes, or accepts it. The clean discharge receipt is what the **witness axis** pins.
(Z3/CVC5/Vampire/CeTA/Lean/Maude/Coq — `provekit-verifier/src/solvers/`.)

## LSP — DERIVED FROM T ("red squigglies when they violate a contract at compile time"), confirm

The per-language editor face that DELIVERS the contract to the user. It sees the boundaries
(sugar usages) in the user's code, knows the contract each sugar carries, and surfaces
violations live as diagnostics. (Per-kit `provekit-lsp-<lang>`.)
- OPEN: does the LSP drive the solver live at the boundary, or is the squiggle a cheaper
  structural check with full solver discharge at verify/CI time? (T to settle.)

## Three axes of pinning (paper 03 §8; docs/security/what-binaryCid-does-not-catch.md)

A proof bundle binds **three independent CIDs**, each pinnable (frozen) or floatable
(track latest acceptable at verify time) → 2³ = **8 trust postures**:
1. **Contract** — what it conforms to. (Pinned: contract identity stable across re-attestation.)
2. **Witness** — the chain that endorses it. (Pinned: the contract is what it claims to be.)
3. **Binary** — what it asserts about. (Pinned: the binary you run is the one the witness was minted against.)

The substrate **picks none of it** — the consumer/vendor decides per axis (security team:
tight witness; dev team: tight binary; compliance: all three). This is *why* concept
equivalence / trust is the vendor's call. Together the three axes close the supply-chain
attack class (authenticated betrayal): a correctly-signed package still can't swap
behavior, forge endorsement, or swap bytes without breaking a pin.

## Loss record — IN SCOPE (100%, T)

Produced **per emitter / per materializer / per solver**. The honest accounting of
lossiness: when emitting a contract, materializing a sugar, or discharging against a
solver cannot be **exact**, the loss is **recorded** — content-addressed and named, not
silent. This is what makes the trichotomy honest — *exact / loudly-bounded-lossy /
refuse* — because "loudly-bounded-lossy" is only honest if the bound is written down.
Silent loss would be a lie; the loss record is the substrate keeping correctness-above-all.
(Lift is not in this list — lift is the singular truth-ingestion; it refuses or
sugar-carries rather than loses.)

## Catalog (concept/realization) — FICTION

There is no central concept/realization catalog. A concept is a shared tag nobody owns;
everything a catalog would hold already lives, content-addressed + signed, in vendor
`.proof`s (paper 23). "The catalog" is at most the **ephemeral union of the vendor .proofs
a consumer has resolved** (`union(resolve(dependencies).proof)`) — a query/view at resolve
time, per-consumer, never a stored global object. A central catalog is the registry
anti-pattern this content-addressed, vendor-pinned substrate exists to abolish.
- INFECTION (delete): the central `menagerie/concept-shapes/catalog/realizations/` store
  the realizer walks up to find. Replace with: resolve the dependency .proofs, union them.

## Protocol record (currently MISNAMED "protocol catalog") — REAL

A living, signed, content-addressed artifact: the record of the **protocol's own
evolution** (lift-plugin protocol, PEP transitions, IR version, extension surface). NOT a
registry of vendor content — the substrate's own ledger of how its rules changed.
"Catalog" mis-frames a living *record* as a static *index*. **Rename: protocol record.**
- Re-signing it on a protocol change (e.g. tonight's `agent-plugin-protocol` de-list) is
  legitimate record-keeping, NOT feeding the fiction. Keep the artifact; fix the name.

## Everything is config — lift/lower dispatch to a plugin roster

The substrate ships **two translation verbs** and **no hardcoded translators**. Which
lifters and lowerers run is declared in `config.toml` (today: `.provekit/lift/*/manifest.toml`,
each a `command` that spawns a kit over RPC).
- Want to lift contracts from a new idiom? Add a **contract-lifter** to config.
- Want to lift sugar? Add a **sugar-lifter**. Same on lower.
`contract` vs `sugar` is a **plugin role declared in config**, never a verb. The substrate
(rust, post-RPC) is language-blind: it runs `lift`/`lower` and routes to whatever the roster
declares. **The kit roster *is* config.toml.**

Lift was already config-driven (the lift manifests). The disease was that **lower never got
the same treatment** — so it grew hardcoded verbs (`emit`, `materialize`, `transport`,
`migrate`) instead of a config roster. Every egress verb cut in the 2026-05 cleanup was a
lowerer that should have been a config line.

## Composition — the Unix nature

The verbs are **single-purpose and compositional**, Unix-style, and the **pipe is the
content-addressed `.proof`** (and the IR/contract stream under it):
- `lift` writes IR; `mint` **tastes that IR** and writes a signed `.proof`; `verify`
  **discharges the contracts the lifter loaded** and writes verdicts + a witness; `lower`
  reads the same IR and writes native.
- No verb reaches into another — they **meet at the artifact**. Content-addressing is what
  makes the pipe trustworthy: the IR `mint` tasted is provably the IR `lift` produced,
  because the CID says so. **Unix pipes with integrity welded in.**

This is why most "new verbs" are a mistake — **a composition frozen into a primitive:**
- `migrate` = `lift | lower`-to-another-library.
- `transport` = `migrate` with paperwork = still `lift | lower`.
- `catalog` = `union(.proof)` — a reduction, not a command.

Ship orthogonal primitives; let the `.proof` be the pipe; anything that looks like a new
verb is almost always two old verbs and a config line. (`compose` survives the cull
precisely because it is **not** a composition — it's a genuine `libprovekit` primitive the
pipe exposes.)

## CLI

A **small set of composable verbs over content-addressed artifacts**, Unix-natured:
- **Translation:** `lift`, `lower` — two verbs, dispatching to a config-declared plugin
  roster over RPC. The substrate is language-blind; **all kits speak one RPC language**; the
  kits resolve their own `.proof` (jar / pip / cargo / classloader) and feed the rust CLI.
- **Substrate algebra (the trinity):** `mint`, `verify`, `witness`, `implicate` over
  {terms, contracts, implications} — not native↔native translation; the substrate's own
  operations, composing through the same `.proof` currency.
- The CLI handles **all computation over the data**; kits do language-specific work behind
  the RPC line.

Anything beyond these that looks like a verb is a frozen composition (above) and belongs as
a pipeline + config, not a command.

_(awaiting next term)_
