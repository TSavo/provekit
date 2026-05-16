# Canonical term_shape form for cross-language federation

**Date:** 2026-05-16
**Status:** LOCKED. Substrate-level ruling. All current and future lift kits emit this canonical form.
**Authority:** T Savo (architect ruling), per Kit's γ defense and advisor independent review.

## TL;DR

The canonical bind-payload `term_shape` for an operation is:

```json
{ "concept_name": "concept:<name>", "op_cid": "blake3-512:<hash>", "args": [<sort markers>] }
```

Where each operand slot in `args` is a bare empty object `{}`.

No surface syntax. No language-specific operator literals (`op:"+"`). No language-specific operation kind tags (`kind:"bin"`). No UNNAMED-CONCEPT-N wrappers in green state. No source-location metadata in the bind payload.

This ruling supersedes the lift-kit-specific term_shape conventions that produced the seam 4 federation byte-identity divergence empirically surfaced 2026-05-16 (PR #1082, Trinity census slow lane).

## Why this matters

Paper 16 (Universal Address Space) establishes federation by content-addressed identity. The substrate's first principle (Supra omnia, rectum) demands that two byte-identical algebras across any two federated languages produce byte-identical bind CIDs. The bind CID is computed over the JCS-canonical bytes of the bind payload BEFORE deserialization (see `libprovekit::core::bind::bind_term_document` and `libprovekit::canonical::serializable_jcs`).

Therefore: every byte in the bind payload that the CID hashes must encode concept-level structure, not lift-side surface syntax. Surface syntax differs across languages by definition; encoding it in the CID hashes a coincidence rather than an algebra.

## The empirical instance that surfaced the question

Antibody flip PR #1082 (`seam4_federation_rust_vs_python_lift_bind_byte_identity`) failed with divergent bind CIDs for the identity algebra `add(x: i64, y: i64) -> i64 { x + y }` lifted from both Rust and Python sources. Architect diagnostic (Explore agent, 2026-05-16) enumerated seven divergence axes in the 244-line JCS-byte diff (`/tmp/a15-byte-diff.txt`, ephemeral; the test source is the durable artifact).

The seven axes:

1. `file` (source path): `src/lib.rs` vs `src/lib.py`. Trivial canonicalization.
2. `term_shape_cid`: downstream hash of axes 3-6. Closes when 3-6 close.
3. `term_shape.stmts[0].kind`: Rust says `"bin"`; Python wraps as `"exit"` with nested `value:`. Language-specific.
4. `term_shape.stmts[0].value.args[]`: Python emits two `{"kind": "opaque"}` operand markers; Rust omits.
5. `term_shape.stmts[0].value.concept_name` + `op_cid`: Python embeds inline; Rust omits.
6. `named_term_tree`: structural divergence in operation_kind and args.
7. Path leakage in named_term_tree leaves.

Two of the seven (1, 7) are trivial. One (2) is downstream of 3-6. Four (3, 4, 5, 6) reduce to a single architectural question: sparse vs expanded canonical representation. This document answers that question.

## The three options enumerated

### Option α: sparse-Rust canonical

Both sides emit Rust's current form: `{"kind": "bin", "op": "+"}` only, no concept embedding in term_shape. Concept resolution lives outside term_shape (in named_term_tree or sibling fields).

**Status: rejected.**

Failure mode: encodes language-specific surface syntax (`kind:"bin"`, `op:"+"`) in the bind CID hash. Federation breaks the moment a language enters with different surface for the same concept:

- Rust uses `&&` / `||` for logical and/or; Python uses keywords `and` / `or`. Even today's two-language federation is broken for these operators.
- Future languages: Lua `+` vs Lisp `(+ a b)` vs APL `+` vs Java `Math.addExact(a, b)`. All these surface forms must produce identical bind CIDs for concept:add. Sparse form makes that impossible.

α only "works" for Python+Rust on `+` by accident of shared surface syntax. It is not the colimit; it is a test artifact of choosing a friendly first language pair.

### Option β: expanded-Python canonical

Both sides emit Python's current form: `{kind:"bin", op:"+", args:[{kind:"opaque"}, {kind:"opaque"}], concept_name:"concept:add", op_cid:"..."}`. Concept embedding present; surface syntax also present.

**Status: rejected.**

Failure mode: same as α at scale. The surface-syntax fields (`kind:"bin"`, `op:"+"`) still leak into the bind CID hash. β works for `+` between Python and Rust by the same accident-of-shared-syntax as α, and breaks at the same operators (and/or where Python uses keywords and Rust uses `&&`/`||`).

β is α-with-extra-information. The extra information (concept_name + op_cid) is what federation needs; the surface syntax is what federation breaks on. β is a half-step from α; not the colimit.

### Option γ: concept-centered, surface-stripped (LOCKED)

The minimum canonical form is:

```json
{
  "concept_name": "concept:<name>",
  "op_cid": "blake3-512:<hash>",
  "args": [{}, {}, ...]
}
```

Surface syntax stripped. Concept resolution embedded. Operand slots are bare empty objects. Composition structure preserved via nesting.

**Status: locked.**

Why γ survives M+N federation: the bind CID hashes only concept-level structure. Two byte-identical algebras across any two languages produce byte-identical bind CIDs because the language-specific surface syntax is no longer in the hashed bytes. The federation property holds by construction, not by accident.

## The exact canonical shape

### Operation term_shape entry

```json
{
  "concept_name": "concept:<name>",
  "op_cid": "blake3-512:<64-hex-chars>",
  "args": [<operand_slot>, <operand_slot>, ...]
}
```

Fields:
- `concept_name`: the concept atom's canonical name (e.g., `concept:add`, `concept:conditional`, `concept:eq`).
- `op_cid`: the BLAKE3-512 CID of the concept atom's spec. Mints once per concept; identical across languages.
- `args`: array of operand slots. Length = operation arity. Order = positional.

### Operand slot

```json
{}
```

Bare empty object. Operand arity is encoded by array length. Operand sort is derivable from function-level signature (`param_types`, `return_type` at the function level, already locked by A9) plus the operation's own sort signature.

### Composition (nested operations)

An operand slot whose value IS an operation is the operation's term_shape entry, not a bare `{}`:

```json
{
  "concept_name": "concept:add",
  "op_cid": "...",
  "args": [
    { "concept_name": "concept:mul", "op_cid": "...", "args": [{}, {}] },
    {}
  ]
}
```

This expresses `add(mul(x, y), z)`: the first arg is the nested mul; the second is a bare operand slot. Bind CIDs over this nesting are byte-identical across languages because nothing language-specific appears in the hashed bytes.

## Forbidden fields

The following must NOT appear in term_shape (in any language's lift kit):

- `kind: "bin"` or any other kind tag from lift-side syntax
- `op: "+"` or any other operator literal from lift-side syntax
- Any `op_cid` field at a position other than the operation root (operands carry no op_cid)
- Source-location metadata (`file`, `line`, `column`, `fn_line`). See section on source-location below.
- `concept_annotation`, `attr_pre`, `attr_post`, `concept_citations`. These are lift-side scaffolding or relift-carrier flow; not bind-tier.
- UNNAMED-CONCEPT-N wrappers. These are transient diagnostic state for unresolved concepts. The green state (all concepts resolved) has no UNNAMED-CONCEPT entries. Lift kits emit refusal mementos for unresolved positions, not UNNAMED-CONCEPT placeholders in term_shape.

## Source-location metadata

Source location (`file`, line, column, function-name source position) is not substrate-level. The bind CID must not vary based on which workspace path a source happens to live at, nor which line a function starts on.

Lift kits must NOT include source-location in the bind payload. If source-location provenance is needed for debugging or error reporting, it belongs in a separate provenance memento that lives alongside the bind payload but is NOT hashed into the bind CID.

## Deferred extension: polymorphic operations

The bare `{}` operand slot suffices for all current concept operations (A10-minted: add/sub/mul/div/eq/ne/lt/le/gt/ge/and/or/not + bonus mod/shl/shr/bitand/bitor/bitxor/neg/bitnot). Each of these is monomorphic-per-application: per-operand sort is derivable from function-level signature plus the operation's own sort signature.

When polymorphic concept operations are introduced (type-class methods, generics over multiple sorts where per-operand sort is NOT a function of function-level signature plus op signature), the bare `{}` operand slot becomes insufficient. At that point the canonical operand slot extends to:

```json
{ "sort": "concept:<sort-cid>" }
```

Where `concept:<sort-cid>` is a concept-tier sort identifier (NOT a language-level type like `int` or `i64`). This preserves A9's type-erasure invariant: surface types are never in the bind payload; concept-tier sorts may be when polymorphism demands it.

This extension is deferred until the first polymorphic concept operation lands. Document the deferral here so future kits know when to revisit.

## Cross-references

- Paper 16: docs/papers/16-after-x-the-universal-address-space.md
- A9 (type-erasure at lift boundary): #1075, PR #1078 (merged 2026-05-16)
- A14 (deep-nested operator preservation): #1083, PR #1085 (paused, redesign required per this ruling)
- A15 (federation byte-identity): #1084 (locked-scope deletion of concept_citations was correct but insufficient; redesign required per this ruling)
- Antibody flip PR: #1082 (verification artifact; rebases when A14 + A15 + remaining axes close per this canonical form)
- Trinity exhibit (parent): #1024
- Trinity completion checklist: docs/plans/2026-05-16-trinity-completion-checklist.md

## Implementation guidance for A14 and A15 redispatches

### A14 redesign brief (next dispatch)

The original A14 closed the UNNAMED-CONCEPT residual via a sparse-term_shape change (`syn::Stmt::Local` routes through `shape_of_expr`; literal/path leaves emit `non_operation_shape()`). Under γ, the sparse form is wrong. The redesign:

- Rust lift's term_shape emitter (`implementations/rust/provekit-walk/src/bin/walk_rpc.rs`) MUST emit the canonical γ shape per this doc.
- The UNNAMED-CONCEPT gap A14 originally targeted is now closed by a position-agnostic operator-naming pass that runs over the whole AST and emits canonical γ for every binary operation encountered, regardless of position (let-RHS, conditional-else, top-level). No syntactic-position special-casing.
- Regression tests stay (let-RHS, top-level discrimination, nested conditional else) but assert against canonical γ output.

### A15 redesign brief (next dispatch)

The original A15 deleted `concept_citations` from `bind_lifter.py`. That deletion was correct (and stays) but insufficient. The redesign:

- Python lift's term_shape emitter (`implementations/python/provekit-lift-python-source/src/provekit_lift_python_source/bind_lifter.py`) MUST emit the canonical γ shape per this doc.
- Drop `kind`, `op`, surface-syntax fields, `exit` wrappers around tail expressions, and UNNAMED-CONCEPT-N wrappers.
- Keep `concept_name` and `op_cid` embedding (these are γ-canonical).
- Strip source-location from bind payload entirely.

### Axes 1 + 7 (source path)

Both Rust and Python lift kits must strip source-location from the bind payload. This is a trivial-canonicalization mechanical change in each lift; can ride along with the A14 / A15 redesign PRs or land separately.

## Pre-condition for declaring Trinity green

The seam 4 federation test (`seam4_federation_rust_vs_python_lift_bind_byte_identity`) produces byte-identical bind CIDs for the identity algebra after A14 and A15 land redesigned per this doc. The `#[should_panic]` marker on antibody flip PR #1082 comes off; the test transitions to clean `assert_eq!`.

The seam 4 discrimination test (`seam4_discrimination_structural_diff_is_captured_when_present`) continues to pass: distinct algebras still produce distinct bind CIDs. The fix narrows the equivalence class to "same concept-level algebra"; it does not collapse legitimate distinctions.

When both hold empirically, Trinity #1024 closes.
