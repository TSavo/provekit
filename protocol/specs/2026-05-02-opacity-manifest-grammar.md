# Opacity Manifest Grammar

**Status:** v1.4.0 normative draft
**Date:** 2026-05-02
**Catalog property:** Listed in the v1.4.0 catalog as `opacity-manifest-grammar`; CID is computed from this file's bytes per the catalog format (`2026-04-30-protocol-catalog-format.md` §2.1, raw-byte BLAKE3-512).
**Owner:** verifier crate + every conformant IR compiler.
**Related:**
- `2026-05-02-ir-compiler-protocol-v2.md` (the emission requirement)
- `2026-05-02-multi-solver-protocol-v2.md` (the consensus rule that consumes manifests)
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization)
- `2026-04-30-proof-file-format.md` (envelope grammar style)

## §0. The protocol is the bytes

An OpacityManifest is structured data. Two compilers in two languages translating the same opaque IR positions in the same way MUST produce byte-identical manifests. The bytes are the protocol; the grammar in this document is the description of the bytes.

A reader who hashes the manifest a different way, or compares manifests by parsing them into language-native objects rather than comparing JCS-canonical bytes, is hashing or comparing something other than the protocol-defined manifest. The verifier's coverage rule is defined in terms of `positionCid` strings, which are themselves byte-derived; everything composes back to the bytes.

## §1. Why opacity manifests exist

The verifier composes verdicts from multiple solvers. A solver that translates IR-JSON into its native theory may encounter positions in the formula it cannot soundly translate (a `kit_predicate` with no theory semantics, a `Lambda` whose body is itself a `Lambda`, a quantifier over predicates, a value-dependent type). The IR is not at fault: the IR is the universal language; *each compiler is the authority on what its theory can soundly handle*.

A v1.3.0 compiler, faced with such a position, has two options:
1. Refuse to emit anything (`compile_error.unsupported_*`).
2. Emit a body that silently elides the offending position.

Option 1 cuts off composition: a position one solver cannot handle removes the entire formula from that solver's contribution to a portfolio. Option 2 is unsound: the solver returns `Discharged` for a formula it never fully reasoned about.

The v1.4.0 design takes a third option. **A compiler emits a tractable script for everything it can soundly translate, marks every untranslated position with a theory-equivalent of "trust me" (SMT `(assert true)`, Coq `Admitted.`, Lean `sorry`, Isabelle `oops`), and records each marked position in an OpacityManifest.** The compiled output remains syntactically valid for the solver. The opaque parts are MARKED, not omitted.

The verifier reads every solver's manifest. If solver A marked position X opaque and solver B (whose manifest does NOT contain X) returned `Discharged` for the same formula, position X is *covered* by B. A consensus that covers every opacity position across the union of manifests is a sound joint discharge: each opaque position was reasoned about by *some* solver in the pool that did not need to mark it opaque.

The OpacityManifest is the data structure that makes this rule decidable and content-addressed.

## §2. Grammar

The OpacityManifest is a JSON object with exactly four required top-level properties.

```ebnf
OpacityManifest ::= "{"
                      "\"protocolVersion\"" ":" "\"ir-compiler-protocol/2\"" ","
                      "\"compiler\"" ":" String ","
                      "\"compilerVersion\"" ":" String ","
                      "\"opacities\"" ":" "[" Opacity ( "," Opacity )* "]"
                    "}"

Opacity ::= "{"
              "\"positionCid\"" ":" PositionCid ","
              "\"reasonCode\"" ":" ReasonCode
            "}"

PositionCid ::= "\"blake3-512:\"" HexDigest
HexDigest   ::= 128 lower-case hex characters

ReasonCode ::= "\"kit_predicate_no_semantics\""
             | "\"nested_lambda\""
             | "\"predicate_quantification\""
             | "\"dependent_type\""
             | "\"other:\"" FreeformReason

FreeformReason ::= String   // arbitrary; documents the reason for tooling
```

### §2.1 Field semantics

| Field | Type | Required | Meaning |
|-------|------|----------|---------|
| `protocolVersion` | string | yes | MUST be the literal `"ir-compiler-protocol/2"`. Tags the manifest with the contract that produced it. A v1.5.0 manifest will use a different tag. |
| `compiler` | string | yes | The compiler's dialect identifier from the IR-compiler dialect registry, e.g. `"smt-lib-v2.6"`, `"gallina"`, `"lean-tactic-mode"`. Identifies which compiler emitted the manifest, not which solver consumes it. |
| `compilerVersion` | string | yes | The compiler implementation's version, surfaced in mementos and reports for provenance. SHOULD match the `version` field returned by `provekit.ir.handshake`. |
| `opacities` | array | yes | Possibly-empty list of `Opacity` records. Empty `opacities: []` is the byte-shape for "this compiler translated every position soundly." |

`Opacity.positionCid` is the BLAKE3-512 of the JCS-canonical bytes of the opaque IR subterm; see §3. `Opacity.reasonCode` is one of the closed-enum strings in §4 or an `other:<freeform>` extension code.

### §2.2 Empty manifest

An empty-opacities manifest is **not optional**; a compiler that handled every position soundly MUST still emit the envelope with `opacities: []`. The presence of the envelope is what tells the verifier this compiler conforms to ir-compiler-protocol/2 at all. Absence of an envelope means the solver is pre-v1.4.0 and is excluded from `coverage_required` consensus by the rule in `2026-05-02-multi-solver-protocol-v2.md`.

**INVARIANT:** A compiler that returns a SolveResult for `coverage_required = true` consensus MUST emit an OpacityManifest envelope, even when `opacities` is empty.

### §2.3 Ordering of `opacities`

The `opacities` array MUST be sorted by `positionCid` ascending (lexicographic over the `blake3-512:<hex>` string). When two entries share a `positionCid` (which can happen if a compiler decides the same syntactic subterm is opaque for two distinct reasons), they are sorted by `reasonCode` ascending as a secondary key. After JCS canonicalization the array form is determined byte-for-byte by these two ordering rules.

**INVARIANT:** Two conformant compilers, given the same set of `(positionCid, reasonCode)` pairs to emit, MUST produce byte-identical `opacities` arrays.

### §2.4 SolverTag authority

**INVARIANT (SolverTag authority):** When a `SolverTag` (`name@version`) appears in both a SolveResult envelope and a memento minted from that result, the envelope's tag is authoritative; mementos copy the tag verbatim, never derive their own.

## §3. Position content-addressing

`Opacity.positionCid` is the BLAKE3-512 of the JCS-canonical bytes of the opaque IR subterm:

```
positionCid = "blake3-512:" || hex(BLAKE3-512(JCS(opaque_subterm)))
```

The "opaque subterm" is the IR-JSON node the compiler chose to mark opaque. It may be a single `Lambda` node, a single `Atomic` predicate application, or a sort declaration. The compiler's choice of granularity is the compiler's authority; the verifier compares positions by `positionCid` only and never reasons about the node's internal structure.

**INVARIANT (positional content-addressing):** Two compilers that mark the same syntactic IR subterm opaque produce the same `positionCid`. Two compilers that mark different subterms opaque (even if the difference is a child node nested inside the other) produce different `positionCid`s.

Rationale: this is precisely the "formula equality is hash equality" rule from `2026-04-30-handshake-algorithm.md` applied to opaque subterm comparison. The verifier needs to ask "did some other solver in the pool *not* need to mark this exact position opaque?" Position equality is hash equality of the canonical-bytes representation.

### §3.1 Why JCS over the IR subterm

The canonical IR-JSON grammar (`2026-04-30-ir-formal-grammar.md`) already produces canonical IR bytes. Hashing a node *inside* a canonical document is the same as JCS-canonicalizing the node in isolation, since JCS canonicalization is structural and key-local: the canonical form of a sub-object is the sub-object's canonical form, regardless of its parent context.

Implementations MAY skip a redundant JCS pass if they extract the subterm's bytes directly from a canonical IR document. The bytes hashed are what matter; the route is at the implementer's discretion.

## §4. Reason codes (closed enum + extension procedure)

The four base reason codes shipped in v1.4.0 and the open-bucket extension shape:

| Reason code | Meaning |
|---|---|
| `kit_predicate_no_semantics` | The opaque subterm is an `Atomic` predicate application whose name is registered as a kit predicate, but the compiler has no theory semantics for it. The kit predicate exists in the IR, but the compiler cannot translate its meaning into its target theory. |
| `nested_lambda` | The opaque subterm is a `Lambda` whose body contains another `Lambda`. The compiler's lambda fragment is first-order; a nested-lambda body is outside its sound translation range. |
| `predicate_quantification` | The opaque subterm is a `Lambda` whose `paramSort` is `Bool` or a function sort. The compiler can quantify over individual values but not over predicates. |
| `dependent_type` | The opaque subterm references a value-dependent type (a sort whose definition mentions a value-level variable). The compiler's type system is non-dependent. |
| `other:<freeform>` | Anything else. The freeform suffix after the colon is compiler-specific; it documents the reason for human auditors but participates in the byte-level equality of the manifest. |

**INVARIANT (closed-enum stability):** The four base reason codes (`kit_predicate_no_semantics`, `nested_lambda`, `predicate_quantification`, `dependent_type`) are stable across the lifetime of `ir-compiler-protocol/2`. Conformant compilers MUST emit one of these four codes whenever their reason matches one of these four meanings, rather than falling through to `other:`.

**Extension procedure.** A future minor version of `ir-compiler-protocol/2` (say, `ir-compiler-protocol/2.1`) MAY add additional base codes by amending this spec. New codes are added additively; no existing code is renamed or removed. A v2 verifier consuming a v2.1 manifest treats unrecognized base codes as if they were `other:<code>` for coverage-comparison purposes: the manifest is still valid, but the verifier cannot collapse the new code's semantics into a known equivalence class. This is graceful: a future opacity class that the verifier does not yet understand is treated conservatively.

**INVARIANT (forward compatibility):** A v1.4.0 verifier consuming an OpacityManifest that contains a reason code outside the v1.4.0 set MUST NOT reject the manifest. It MUST treat the unrecognized code identically to `other:<code>`: include the position in the coverage union, require some other solver in the pool to discharge it.

## §5. Coverage-comparison algorithm

The same algorithm is restated normatively in `2026-05-02-multi-solver-protocol-v2.md` §verdict-composition; this spec gives the byte-level form.

Given a set of solvers `S = { s_1, ..., s_n }` each returning a SolveResult `(verdict_i, manifest_i)`:

```
coverage_union = ⋃_{i=1..n} { entry.positionCid | entry ∈ manifest_i.opacities }

is_covered(position_cid) :=
  ∃ i ∈ {1..n} :
    verdict_i = Discharged ∧
    ¬∃ entry ∈ manifest_i.opacities : entry.positionCid = position_cid

consensus_holds := ∀ p ∈ coverage_union : is_covered(p)
```

**INVARIANT (coverage soundness):** If `consensus_holds = true` and every solver's verdict is in `{Discharged}` (no `Unsatisfied`, no missing manifest), then for every position in the coverage union there exists at least one solver in the pool that reasoned about that position soundly and concluded the formula is `Discharged`. Composing those per-position discharges gives a joint discharge of the whole formula.

The full consensus rule (which also requires no solver to have returned `Unsatisfied`, no FOL-fragment disagreement, etc.) lives in the multi-solver-protocol/2 spec; this section defines only the position-level coverage primitive.

## §6. Cross-language byte conformance

A conformant OpacityManifest emitter in any language (Rust, C++, Python, OCaml, ...) MUST produce byte-identical manifests for byte-identical opacity sets. The recipe:

1. Compute each `positionCid` via JCS-canonicalize-then-BLAKE3-512 of the opaque subterm bytes (§3).
2. Build the in-memory `opacities` list, deduplicating by `(positionCid, reasonCode)` if the compiler accidentally records the same opacity twice.
3. Sort the list per §2.3 (positionCid ascending, reasonCode tiebreak).
4. Construct the JSON object with the four required keys.
5. JCS-canonicalize the object per `2026-04-30-canonicalization-grammar.md`.
6. The byte string from step 5 is the manifest's wire form. The manifest's CID, when needed (e.g., for inclusion-by-reference in proof envelopes), is `blake3-512:<hex(BLAKE3-512(jcs_bytes))>`.

A reference test suite at `tests/opacity-manifest-fixtures/` (v1.4.0) provides input/output pairs: given a fixed IR document and a fixed opacity-marking strategy, the expected manifest bytes are checked in. New language ports MUST pass every fixture byte-for-byte.

**INVARIANT (cross-language determinism):** Two conformant OpacityManifest emitters in any two languages produce byte-identical manifest output for the same logical opacity set.

## §7. Inclusion in SolveResult envelopes

Per `2026-05-02-multi-solver-protocol-v2.md` §SolveResult-envelope, every SolveResult emitted by a v1.4.0 solver carries its OpacityManifest as a sibling field of the verdict:

```json
{
  "verdict": "Discharged",
  "opacityManifest": { ... },
  "solver": "z3@4.13.0",
  "wallClockMs": 142
}
```

The `opacityManifest` field's bytes are the JCS-canonical form per §6. The SolveResult envelope itself is JCS-canonicalized once when the verifier wraps it for memento provenance; the manifest's nested JCS form survives the outer JCS pass because JCS canonicalization is structural and idempotent on already-canonical sub-objects.

**INVARIANT:** A SolveResult's `opacityManifest` field, extracted and re-canonicalized in isolation, hashes to the same `blake3-512:<hex>` value as it does inside the SolveResult envelope. JCS does not re-order or alter already-canonical sub-objects.

## §8. Worked example

Consider the formula:

```
forall P. ∀x. P(x) ∧ proves_by_induction(P)
```

In the IR this is two nested `Lambda` nodes (the outer one binding `P` of sort `Bool -> Bool`, the inner binding `x`), with an `And` whose right branch is an `Atomic` application of the kit predicate `proves_by_induction` to the bound predicate variable `P`.

### §8.1 SMT-LIB v2.6 compiler

The SMT compiler:

- Walks the outer `Lambda`. Its `paramSort` is a function sort (`Bool -> Bool`). The compiler marks this `Lambda` opaque with `reasonCode = "predicate_quantification"`. It emits `(assert true)` for that position and continues.
- The inner `Lambda` (`x : Int`) is fine. The compiler translates it as a universally quantified Int variable.
- The `Atomic` application of `proves_by_induction(P)` is a kit predicate the compiler has no semantics for. Marked opaque with `reasonCode = "kit_predicate_no_semantics"`.

Resulting SMT manifest (after JCS):

```json
{
  "compiler": "smt-lib-v2.6",
  "compilerVersion": "0.2.0",
  "opacities": [
    { "positionCid": "blake3-512:5a...", "reasonCode": "kit_predicate_no_semantics" },
    { "positionCid": "blake3-512:c1...", "reasonCode": "predicate_quantification" }
  ],
  "protocolVersion": "ir-compiler-protocol/2"
}
```

(Keys appear sorted by JCS rule. `opacities` entries appear sorted by `positionCid` ascending.)

### §8.2 Coq compiler

The Coq compiler has a higher-order type system and a tactic library that includes `proves_by_induction`. It translates both `Lambda`s natively as `forall` quantifiers (inner: over `nat`, outer: over `(nat -> Prop)`). The kit predicate is registered with a Coq tactic mapping. Nothing is opaque.

Resulting Coq manifest:

```json
{
  "compiler": "gallina",
  "compilerVersion": "0.1.0",
  "opacities": [],
  "protocolVersion": "ir-compiler-protocol/2"
}
```

### §8.3 Coverage check

Both solvers return `Discharged` (the SMT solver because `(assert true)` makes the obligation vacuous; the Coq solver via the actual induction tactic). The verifier:

1. Computes `coverage_union = { 5a..., c1... }`.
2. Checks each position:
   - `5a...` (the kit predicate position): SMT marks it opaque; Coq does not. Coq returned `Discharged`. **Covered.**
   - `c1...` (the outer Lambda position): SMT marks it opaque; Coq does not. Coq returned `Discharged`. **Covered.**
3. Consensus holds. The verifier records the joint verdict as `Discharged`, with provenance: position `5a...` covered by Coq, position `c1...` covered by Coq. The SMT solver's contribution is recorded but is not load-bearing for those two positions.

If only the SMT solver had been in the pool, the coverage check would have failed (no other solver in the pool returned `Discharged` while not also marking those positions opaque). The verdict would be `Undecidable` rather than the unsound `Discharged` an SMT-only run would have produced.

## §9. Failure modes the manifest does NOT prevent

The manifest is a soundness mechanism for *position-level* opacity. It does not prevent:

- **Two solvers disagreeing on the FOL fragment they both reasoned about.** The multi-solver-protocol/2 disagreement check (inherited from v1) handles this orthogonally; if SMT says `Discharged` and Coq says `Unsatisfied` for the same formula's transparent fragment, that is a SOLVER DISAGREEMENT regardless of opacity.
- **A compiler maliciously claiming opacity for positions it could have translated.** The verifier trusts each compiler's manifest; a buggy or adversarial compiler that over-reports opacity will simply require the pool to include another solver that handles those positions, or the formula will be `Undecidable`. Under-reporting opacity (claiming a position is sound when it isn't) IS a soundness bug; the catalog-signed compiler implementation is the trust anchor.
- **A subterm that two compilers disagree on the granularity of.** Compiler A may mark a whole `And` opaque while compiler B marks only the right branch opaque. The two manifest entries have different `positionCid`s and the verifier treats them as distinct positions; covering both requires some solver to handle each. This is conservative: the verifier never assumes A's coarser opacity covers B's finer opacity.

## §10. Acceptance

This spec is satisfied by:

- A reference Rust implementation in `implementations/rust/provekit-opacity-manifest/` (v1.4.0).
- The byte-fixture suite at `tests/opacity-manifest-fixtures/`.
- A conformance test that the SMT-LIB v2.6 compiler emits the manifest expected by §8.1 byte-for-byte on the §8 input.
- Cross-language conformance (when a second-language port lands): both languages produce identical manifest bytes for every fixture.

## §11. Related specs

- `2026-05-02-ir-compiler-protocol-v2.md` — defines the *requirement* that compilers emit this manifest; this spec defines the manifest's *shape*.
- `2026-05-02-multi-solver-protocol-v2.md` — defines the *consumption* rule that uses manifests to compose verdicts.
- `2026-04-30-canonicalization-grammar.md` — JCS canonicalization, normative.
- `2026-04-30-ir-formal-grammar.md` — the IR-JSON grammar whose subterms are content-addressed by `positionCid`.
- `2026-04-30-handshake-algorithm.md` — the "formula equality is hash equality" composition principle this spec extends to position equality.
- `2026-04-30-multi-solver-protocol.md` (v1, superseded) — predecessor without opacity awareness; CID `blake3-512:71fc7ac22997938629d835f87e4e8a322026d77c1e1f834c9fbe0f79cca4e903792c628e96d3004c88d29706f4d87bc042ff837fef571c0cb3012495a03003d3` per the v1.3.0 catalog.
