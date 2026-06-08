# Algorithm Memento Protocol (AMP)

**Version:** v0.1.0 (draft)
**Date:** 2026-05-09
**Status:** design draft for review
**Author:** T Savo
**Companion specs:** PPP (2026-05-09-pattern-predicate-protocol.md), CCP (2026-05-09-contract-composition-protocol.md), FRP (Fix Receipt Protocol), PEP (Protocol Evolution Protocol)

## §0 Why this spec exists

The substrate's first axiom is *Supra omnia, rectum* — above all, correctness.

Today, the substrate's own production mechanism — the LIFTERS that emit contracts — violates that axiom. The same algorithm (e.g. "if/else two-armed WP narrowing") is implemented independently in `sugar-walk` (Rust), `sugar-walk-c` (C/libclang), `sugar-walk-py` (Python AST), `sugar-walk-java` (JavaParser), and `sugar-walk-zig` (Zig AST). Five copies of the same transformation, with no canonical reference, no mechanically-detectable drift, no content-addressed identity for the algorithm itself.

The substrate is the place where claims about behavior settle once and apply everywhere. **Until the substrate's own algorithms settle once and apply everywhere, the substrate fails its own first axiom.**

This spec closes that gap. It defines:

1. The **algorithm memento** — a content-addressed canonical representation of a single transformation
2. The **binding-claim memento** — a verifiable assertion that a piece of language-specific code implements an algorithm memento
3. The **refinement relation** between bindings and their algorithm memento
4. The **discharge protocol** for verifying bindings against algorithms
5. The **federation rule** — what it MEANS for two language ports to "implement the same algorithm"

After AMP lands, the substrate hosts not just claims about user code but claims about its own production mechanism. The lifters become substrate participants. Drift becomes mechanically detectable. New patterns mint once and federate naturally.

## §1 Definitions

### §1.1 Algorithm

An **algorithm** is a deterministic transformation `A : Input → Output`. In the substrate, all algorithms have the shape:

```text
A : ASTPattern × Context → IrFormula
```

where `ASTPattern` is a structural description of an input syntactic shape (independent of any language's concrete AST), `Context` is the surrounding scope/state at the point of the pattern's match, and `IrFormula` is the canonical ProofIR formula language.

Examples of algorithms in the current substrate's lifters:
- `IF_THEN_FAIL_FAST`: matches `(cond, fail_op)` ASTs; emits `¬cond` precondition
- `TWO_ARMED_CONDITIONAL`: matches `(cond, then_branch, else_branch, join_var)` ASTs; emits `(cond → WP[expr_then/var]) ∧ (¬cond → WP[expr_else/var])`
- `SWITCH_CASE_NARROWING`: matches `(scrutinee, [(case_value, arm)])`; emits guarded WP per case
- `LET_BINDING_BACKWARD_SUBSTITUTE`: matches `(binder, rhs, succeeding_WP)`; emits `succeeding_WP[rhs/binder]`
- `INDIRECT_CALL_RESOLUTION`: matches `(callee_expr, args, scope)`; resolves callee or emits opacity entry
- `ASSERTION_AS_CALLSITE_CONTRACT`: matches `(assertion_macro, args, callsite)`; emits ContractDecl on the called function at the callsite

### §1.2 Algorithm Memento

An **algorithm memento** is the canonical content-addressed description of an algorithm. It is a `FunctionContractMemento` (per CCP §2) over the abstract function `A` defined in §1.1, with conventions on what the formal contract describes.

```text
AlgorithmMemento ⊆ FunctionContractMemento where:
  - fn_name           : the algorithm's canonical short name (e.g. "if-then-fail-fast")
  - formals           : ["ast_pattern", "context"]
  - formal_sorts      : [ASTPattern, Context]
  - return_sort       : IrFormula
  - pre               : the AST shape recognizer (a predicate over ast_pattern that says "this algorithm fires on these inputs")
  - post              : the transformation specification (a formula that defines the output IrFormula in terms of ast_pattern and context)
  - effects           : EffectSet — the algorithm's effects (typically pure: ∅; some algorithms emit side-band opacity entries)
  - locus             : OPTIONAL — pointer to a reference implementation if one exists
  - body_cid          : OPTIONAL — CID of a reference executable form (Coq function, WASM module, lambda-calculus term, ...) if one exists
  - auto_minted_mementos : EMPTY
```

The algorithm's identity is the BLAKE3-512 CID of this memento (canonicalized via JCS per CCP §3).

### §1.3 Binding-Claim Memento

A **binding-claim memento** is a verifiable assertion that a specific piece of language-specific code implements a specific algorithm memento. It is also a `FunctionContractMemento`, with conventions:

```text
BindingClaimMemento ⊆ FunctionContractMemento where:
  - fn_name           : "<algorithm_short_name>:<language>:<version>"
                        (e.g. "if-then-fail-fast:c-libclang:0.1.0")
  - formals           : ["language_ast_input", "context"]
  - formal_sorts      : [LanguageAST_<lang>, Context]
  - return_sort       : IrFormula
  - pre               : language-specific AST shape recognizer
  - post              : binding refines algorithm CID X — see §2
  - effects           : the binding's effects (typically pure for thin bindings)
  - locus             : pointer to the binding code (file:line:col)
  - body_cid          : CID of the binding source code per the §4 normalization rule
  - auto_minted_mementos : EMPTY
  - input_cids        : [<algorithm_cid>, <projection_memento_cid>] - must include both the bound algorithm CID and the language projection (P_lang) CID
```

A binding-claim memento ASSERTS that, for any input the binding's `pre` matches, the binding produces an output equal to what the algorithm memento's `post` would produce on the corresponding `ASTPattern` (the language-AST projected to the canonical AST shape). See §2 for the formalization.

### §1.4 Refinement

A binding-claim memento `B` REFINES an algorithm memento `A` iff:

1. **Pre coverage:** `B.pre` is a non-empty restriction of `A.pre` projected to `B`'s language. Every input `B` accepts MUST be expressible as an instance of `A`'s `ASTPattern` after the language-AST projection.
2. **Post equality:** for every input `i` accepted by both `B.pre(i)` and `A.pre(project(i))`, `B(i) = A(project(i))`. The binding's output equals the algorithm's output on the projected input.
3. **Effect compatibility:** `B.effects ⊆ A.effects`. The binding may not introduce effects the algorithm doesn't sanction.

The `project` function maps the language-specific AST to the canonical `ASTPattern`. Per language, `project` is itself a content-addressed object (a small projection memento) — see §6.

### §1.5 Algorithm Catalog

The **algorithm catalog** is a content-addressed collection of algorithm mementos, signed under the foundation key. Entries are added by minting (see §3) and refined by issuing successor mementos with explicit `refines` links.

```text
AlgorithmCatalog := {
  algorithms: { algorithm_cid → AlgorithmMemento },
  bindings:   { (algorithm_cid, language) → [BindingClaimMemento] },
  projections: { language → ASTProjectionMemento }
}
```

The catalog's CID is the BLAKE3-512 of its JCS encoding.

## §2 The refinement relation, formally

Given:
- `A` : algorithm memento with `A.pre : ASTPattern → Bool` and `A.post : ASTPattern × Context → IrFormula`
- `B` : binding-claim memento with `B.pre : LangAST → Bool`, `B.post : LangAST × Context → IrFormula`, and `B.input_cids ∋ A.cid`
- `P_lang` : projection memento for the language, with `P_lang.project : LangAST → Option<ASTPattern>`

The refinement claim is:

```text
∀ (i : LangAST) (ctx : Context). B.pre(i) →
  ∃ (a : ASTPattern). P_lang.project(i) = Some(a)
                    ∧ A.pre(a)
                    ∧ B.post(i, ctx) = A.post(a, ctx)
```

Encoded as IrFormula in `B.post`:

```text
B.post = forall (i:LangAST) (ctx:Context).
           B.pre(i) → ∃ a:ASTPattern.
             P_lang.project(i) = Some(a)
             ∧ A.pre(a)
             ∧ B(i, ctx) = A(a, ctx)
```

This is the OBLIGATION the binding's verification must discharge. It is itself an IrFormula and feeds the prove portfolio via `sugar lower --to smt-lib | z3 -in` (or coq, or vampire, etc.).

## §3 Minting an algorithm memento

To mint an algorithm `A`:

1. Define `A.pre`, `A.post`, `A.effects` as IrFormula values.
2. (Optional but recommended) Provide a reference implementation in some canonical executable form (Coq term, WASM module, lambda-calculus term). Hash it to `A.body_cid`.
3. Build the FunctionContractMemento per CCP §2.
4. Compute `A.cid` as `BLAKE3-512(JCS(A))` per the canonicalizer.
5. Sign with the foundation v0 key (per existing CCP §6 conventions for self-attestation).
6. Add to the algorithm catalog at `protocol/algorithm-catalog/<A.cid>.json`.
7. Update the catalog's index file with the new entry.

The minted memento is now a substrate citizen. Bindings can refer to its CID.

## §4 Minting a binding-claim memento

To mint a binding-claim `B` for algorithm `A` in language `L`:

1. Identify the binding code (the lifter's source files).
2. Compute `B.body_cid` as `BLAKE3-512(JCS(M))`, where `M = { "files": <array of objects { "path": <repo-relative POSIX path, Unicode NFC normalized>, "content_cid": "blake3-512:" + lowercase-hex(BLAKE3-512(<raw file bytes>)) }, sorted ascending by "path"> }`. This rule is order-independent, separator-free, and byte-reproducible across producers and platforms: file ordering, newline conventions, and path encoding cannot affect the result.
3. Define `B.pre` as the language-AST recognizer matching the same shape as `A.pre`'s `ASTPattern` projection.
4. Define `B.post` per §2's refinement obligation, citing `A.cid`.
5. Set `B.input_cids = [A.cid, P_lang.cid]`.
6. Build the FunctionContractMemento.
7. Compute `B.cid`.
8. Sign with the foundation v0 key (or a delegated key for the language port's maintainer if such a hierarchy emerges).
9. Add to the algorithm catalog under `bindings[(A.cid, L)]`.

## §5 Discharging a binding-claim

The refinement obligation in `B.post` is an IrFormula. To discharge:

1. Lower `B.post` to SMT-LIB (or Coq) via `sugar lower --to <target>`.
2. Run the prove portfolio. For trivial bindings (where `project` is the identity and `B.post` desugars to `forall i. A(i) = A(i)`), the discharge is mechanical.
3. For non-trivial bindings (where the language-AST has shapes the canonical pattern doesn't, or vice versa), the discharge is a genuine theorem to prove. The portfolio's verdict (UNSAT for valid binding, SAT with counterexample for refuted binding) becomes a `BindingDischargeReceipt` memento, signed and stored.

The receipt's CID is the proof that the binding is correct. Without the receipt, the binding is UNATTESTED (lifter output is consumed at the consumer's risk).

## §6 Projection mementos

Each language has an `ASTProjectionMemento`:

```text
ASTProjectionMemento {
  language        : "c" | "rust" | "python" | "java" | "zig" | ...
  ast_library     : "libclang" | "syn" | "ast" | "javaparser" | "std.zig.Ast" | ...
  project_fn_cid  : CID of the canonical project function
  pre_canonical   : recognizer for the source-AST shape that maps cleanly
  post_canonical  : transformation shape that produces the canonical ASTPattern
}
```

The projection memento is a once-per-language artifact. Bindings reference it via CID. Multiple bindings in the same language all use the same projection.

## §7 Federation rule

**Two binding-claim mementos that share the same algorithm CID and pass the discharge protocol implement the same algorithm by the substrate's definition.** Drift between them is mechanically detectable: one will pass the prove portfolio, the other will be refuted.

When a substrate consumer (e.g. the prove pipeline) ingests output from multiple bindings of the same algorithm:

1. Verify each binding's `BindingDischargeReceipt`. Reject output from any unattested binding.
2. Treat the outputs as semantically equivalent — they are, by §2's refinement guarantee.
3. Composition of contracts emitted by different bindings of the same algorithm is mechanical via the existing CCP composition (the algorithm CID is the join key for federation).

## §8 Lifecycle

### §8.1 Refinement

If an algorithm's `pre` or `post` needs to evolve (e.g. a previously-thought-unconditional algorithm turns out to need a guard), a new algorithm memento `A'` is minted with `A'.refines = A.cid`. Existing bindings remain valid against `A`; new bindings may target `A'`. The catalog tracks the refinement chain.

### §8.2 Deprecation

An algorithm memento may be marked deprecated by minting a `DeprecationMemento` referencing its CID. Bindings of deprecated algorithms continue to function but consumers may emit warnings. Removal from the catalog requires a PEP-style protocol evolution (see PEP).

### §8.3 Versioning

The catalog itself is versioned via PEP. Adding new algorithm mementos is an extension-only patch (e.g. v1.7.1). Refining an existing algorithm or changing the binding-claim shape is an extension that may require a minor version (v1.8.0).

## §9 Worked example: TWO_ARMED_CONDITIONAL

### §9.1 The algorithm memento

```json
{
  "fn_name": "two-armed-conditional-wp-narrowing",
  "formals": ["ast_pattern", "context"],
  "formal_sorts": [
    {"kind": "ctor", "name": "ASTPattern", "args": [...]},
    {"kind": "ctor", "name": "Context", "args": [...]}
  ],
  "return_sort": {"kind": "ctor", "name": "IrFormula", "args": []},
  "pre": {
    "kind": "atomic", "name": "matches",
    "args": [{"kind": "var", "name": "ast_pattern"},
             {"kind": "ctor", "name": "TwoArmedConditional", "args": [
               {"kind": "var", "name": "cond"},
               {"kind": "var", "name": "then_arm"},
               {"kind": "var", "name": "else_arm"},
               {"kind": "var", "name": "join_var"}
             ]}]
  },
  "post": {
    "kind": "atomic", "name": "=",
    "args": [
      {"kind": "var", "name": "result"},
      {"kind": "ctor", "name": "And", "args": [
        {"kind": "ctor", "name": "Implies", "args": [
          {"kind": "var", "name": "cond"},
          {"kind": "ctor", "name": "WP_after_substitute", "args": [
            {"kind": "var", "name": "context"},
            {"kind": "var", "name": "join_var"},
            {"kind": "var", "name": "then_arm"}
          ]}
        ]},
        {"kind": "ctor", "name": "Implies", "args": [
          {"kind": "ctor", "name": "Not", "args": [{"kind": "var", "name": "cond"}]},
          {"kind": "ctor", "name": "WP_after_substitute", "args": [
            {"kind": "var", "name": "context"},
            {"kind": "var", "name": "join_var"},
            {"kind": "var", "name": "else_arm"}
          ]}
        ]}
      ]}
    ]
  },
  "effects": {"effects": []},
  "body_cid": null
}
```

The algorithm CID is `BLAKE3-512(JCS(this))`.

### §9.2 The walk-c binding claim

```json
{
  "fn_name": "two-armed-conditional-wp-narrowing:c-libclang:0.1.0",
  "input_cids": ["<algorithm_cid_from_§9.1>", "<projection_memento_cid_for_c-libclang>"],
  "pre": "matches CXCursor_IfStmt with both then-clause and else-clause",
  "post": "for any libclang input matching pre, this binding's output equals the algorithm's output on the projected ASTPattern",
  "body_cid": "BLAKE3-512(implementations/c/sugar-walk-c/src/conditional.c source bytes)",
  ...
}
```

The walk-c source code IS evidence; the binding-claim memento is the formal assertion that it correctly implements the algorithm. The discharge receipt verifies the assertion.

### §9.3 Sibling bindings

When sugar-walk Rust, walk-py, walk-java, walk-zig each mint a binding-claim against the same algorithm CID, the substrate now has 5 bindings of one canonical algorithm. Their outputs federate trivially via the algorithm CID. Drift between them is mechanically detectable via the discharge protocol.

## §10 Relation to existing protocols

- **PPP (Pattern Predicate Protocol):** PPP names how editorial patterns compile to substrate queries. AMP names how the SUBSTRATE'S OWN ALGORITHMS get content-addressed. PPP describes WHAT to look for in lifted substrate; AMP describes HOW the lifters produce that substrate. Composable: a PPP predicate can match the output of an AMP-attested binding with full provenance.
- **CCP (Contract Composition Protocol):** AMP mementos are CCP `FunctionContractMemento` instances with conventions. CCP composition applies. AMP doesn't introduce a new wire format.
- **FRP (Fix Receipt Protocol):** Discharging a binding-claim produces a receipt that participates in FRP's chain.
- **PEP (Protocol Evolution Protocol):** Catalog evolution (adding/refining algorithms) goes through PEP.

## §11 Open questions

The following are intentionally NOT specified in v0.1.0:

- **Canonical executable form** for algorithms (Coq term vs WASM vs lambda-calc). v0.1.0 makes `body_cid` OPTIONAL. Future versions may require it.
- **Projection memento shape** in detail (§6 sketches; needs working out per language).
- **The `WP_after_substitute` and similar constructors** in §9 reference `IrFormula` constructors that don't yet exist in the IR. AMP REQUIRES extensions to ProofIR (per the algebraic-effects design doc) to fully express these.
- **Catalog signing hierarchy** — should language-port maintainers have delegated signing keys, or does foundation key sign all bindings?
- **Discharge automation** — when a binding's source code changes (body_cid bumps), should the discharge re-run automatically? Via what trigger?

## §12 Out of scope (this draft)

- Implementing AMP. This is design only. v0.1.0 is the protocol; reference implementation comes after review.
- Migrating existing lifters to AMP. The migration is a substantial re-architecture; once AMP stabilizes, a migration spec follows.
- The algebraic-effects extensions to ProofIR (separate design doc, in flight).

## §13 Why this matters (the closing principle)

The substrate's first axiom is *Supra omnia, rectum*. A substrate that produces correctness-receipts but cannot itself be content-addressed at the algorithm layer is producing claims about user code while making un-content-addressed claims about its own production mechanism. That is a structural inconsistency.

AMP closes the gap. After AMP lands, every contract emitted by every lifter carries provenance to a content-addressed algorithm + a content-addressed binding-claim + a discharge receipt. Drift is detectable. Federation is mechanical. The substrate hosts its own production.

The substrate finally applies its first axiom to itself.

T Savo
