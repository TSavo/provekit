# CompoundContractMemento + EvidenceMemento -- Normative Spec (PR-A: schema + Rust types)

**Status:** v1.6.x normative-draft (PR-A of a multi-PR landing; see §8)
**Date:** 2026-05-13
**Related:**
- `2026-05-12-concept-site-memento.md` (this spec AMENDS its §1.1 and §5.4; see §0.4 below)
- `2026-05-15-concept-hub-abstraction-layer.md` (§2.1 ConceptAbstractionMemento, §2.4 loss-record dimensions, §2.5 discharge)
- `2026-05-03-substrate-layers-envelope-header-body.md` (envelope/header/metadata layering)
- `2026-05-03-contract-cid-vs-attestation-cid.md` (CID semantics for inter-memento references)
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization, normative)
- `2026-04-30-ir-formal-grammar.md` (IrFormula shape)
- `2026-05-13-wp-as-formula.md` (wp_rule synthesis at clustering-mint time)
- `docs/papers/09-lossy-boundary-compression.md` (obligation-preserving loss; trichotomy precedent)

## §0. Purpose

A real function in a user codebase carries contract evidence from many sources:

1. `#[requires]` / `#[ensures]` annotations (already lifted by `provekit-lift-contracts`).
2. Test assertions targeting the function (e.g., `assert_eq!(f(3), 9)` inside `#[test]`).
3. Type signatures, where the return type itself carries a partial post (e.g., `-> Option<T>` says "may be absent").
4. Docstring contracts (e.g., `/// Returns None if key missing`).
5. Bounded-loop assert invariants already lifted as `LoopInvariantMemento`.
6. `assert!()` / `panic!()` / `unwrap()` / `?` call-sites as implicit pre/post.
7. Native contract surfaces (JML on Java, Zod on TS, Spring annotations, pydantic, OpenAPI).
8. `wp_rule` synthesized structurally from the term algebra at clustering-mint time (for discovered unnamed clusters; per `2026-05-13-wp-as-formula.md`).
9. Empirical witnesses from past runs (`WitnessMemento` instances).
10. Human review comments in PR history mentioning invariants (priority: future).

Each is a typed piece of evidence about what the function does. The substrate already mints `FunctionContractMemento` from source (1). `ConceptSiteMemento.local_contract_cid` (per `2026-05-12-concept-site-memento.md` §1.1) points at exactly one `FunctionContractMemento`. That binding is the convergence point user-code -> catalog. But pinning that field to a single source-shape collapses the many evidence channels prematurely.

This spec defines two new mementos that fix the convergence point:

- `EvidenceMemento`: one piece of contract evidence from one source, content-addressed.
- `CompoundContractMemento`: a content-addressed aggregation of evidences for one function, carrying composed pre/post and a `aggregation_strategy` that derives a single compound verdict from per-evidence verdicts.

The convergence point becomes the compound. `ConceptSiteMemento.local_contract_cid` now points at a `CompoundContractMemento`, not a bare `FunctionContractMemento`. The substrate's binding stays load-bearing across all ten derivation sources.

### §0.1 The trichotomy is by construction at TWO levels

Per Supra omnia, rectum (the 2026-05-11 refinement: "never claim more than you can prove"), the trichotomy `{exact, loudly-bounded-lossy, refuse}` is enforced both per-evidence and at the compound level. Silent contract-dropping is impossible at either level.

- **Per-evidence verdict.** Each evidence has its own verdict when discharged against the concept's `wp_rule`. Verdicts are recorded in the discharge receipt (PR-F), not inside the evidence itself; the evidence is the raw contract bytes, not the discharge.
- **Compound verdict.** The compound's verdict is DERIVED from the per-evidence verdicts under the recorded `aggregation_strategy`. See §2.

A third path to `loudly-bounded-lossy` exists by construction: a binding-target CID that is recorded openly as a `pending:<symbol>` sentinel rather than as a resolved BLAKE3-512 digest. The sentinel's existence IS its contract; the discharger MUST NOT silently treat such a target as resolved. See §10.1 for the admitted form and the discharge consequence.

### §0.2 Why the evidence-set bytes are part of the compound CID

Two compounds aggregating the same function's evidence but drawing from different sources (one with annotations only, one with annotations + tests + docstring) live at different CIDs. Different evidence-sets = different compound bytes = different CID. This is the same principle as ConceptSite §0.2 (different loss-record bytes = different binding CID): different addresses, different things. Adding a new evidence rolls the compound's CID, which rolls the binding's CID, which rolls every downstream consumer that cited that binding. The substrate's address space is its propagation engine.

### §0.3 Evidence is data; verdict is discharge

An `EvidenceMemento` carries a predicate plus its provenance (source kind, source locator, lifter CID). It does NOT carry a discharge verdict. The verdict comes when the compound is discharged against a concept by the wp evaluator + witness sampler (PR-F). The evidence-memento is content-addressed by its data, including `confidence_basis_points` (the lifter's prior on how reliably this source asserts what it claims, e.g., 10000 for static `#[ensures]` annotation, lower for docstring-extracted predicates with grammar ambiguity).

### §0.4 Amendment to 2026-05-12-concept-site-memento.md

The following fields of `2026-05-12-concept-site-memento.md` are AMENDED by this spec:

- **§1.1 field semantics, `local_contract_cid` row.** Was: "The `FunctionContractMemento.cid` of the user-lifted contract for this function." NOW READS: "The `CompoundContractMemento.cid` of the user-lifted compound contract for this function. A pool entry that is a bare `FunctionContractMemento` at this CID is auto-promoted to a single-evidence compound during validation (see §4.4 below); this preserves backward compatibility for bindings minted before the compound layer landed."
- **§5.4 pool-referent constraint, `local_contract_cid` row.** Was: "The pool MUST contain a `FunctionContractMemento` with `cid = header.local_contract_cid`." NOW READS: "The pool MUST contain EITHER a `CompoundContractMemento` with that CID, OR a `FunctionContractMemento` with that CID that the validator auto-promotes per §4.4."

`2026-05-12-concept-site-memento.md` SHOULD carry a one-line footnote on its §1.1 and §5.4 tables citing this amendment for an audit-trail. The footnote MUST NOT alter the original normative text; the amendment lives here.

The `code_site.function_term_cid` field of `ConceptSiteMemento` continues to point at a `FunctionContractMemento` (it is the term-CID of the surrounding function, not a contract source); only `local_contract_cid` changes.

## §1. Wire shape (CDDL, v1.6.x layered)

### §1.1 `EvidenceMemento`

```cddl
; Shared scalar types:
;   hash, cid, signature, pubkey, iso8601, ir-formula, json-value

; Locked JCS key order: end, start
; Both points are {line, col} pairs (1-based line, 0-indexed col in UTF-8
; bytes; see §1.1 normative note below) instead of byte offsets, because
; evidence often comes from formatters / docstrings / tests where absolute
; byte offsets are unstable across re-formatting.
; NOTE: CDDL display order {line, col} is illustrative. JCS sorts keys
; alphabetically, so the on-wire bytes are encoded as {col, line}.
; CDDL display order != JCS canonical order.
source-locator-span = {
  end:   { line: uint, col: uint },
  start: { line: uint, col: uint }
}

; Locked JCS key order: source_cid, span
source-locator = {
  source_cid: cid,                    ; CID of the canonicalized source artifact
  span:       source-locator-span
}

; Open enum of source-kind labels. Validators MUST accept any unknown
; label as a deferred-extension placeholder; downstream consumers DECIDE
; how to treat unknown kinds. The ten canonical labels are listed in §10.
source-kind = tstr                   ; one of the canonical labels OR an extension label

; The evidence-memento itself.
;
; Locked JCS key order (header, alphabetical):
;   cid, confidence_basis_points, extension_fields, kind, lifter_cid,
;   predicate, schemaVersion, source_kind, source_locator
evidence-memento = {
  envelope: {
    declaredAt: iso8601,
    signature:  signature,            ; over JCS(header ++ metadata)
    signer:     pubkey
  },
  header: {
    cid:                       cid,   ; DERIVED -- see §3
    confidence_basis_points:   uint,  ; 0..10000 inclusive; lifter prior
    extension_fields:          { * tstr => json-value },
    kind:                      "evidence",
    lifter_cid:                cid,   ; which lifter emitted this evidence
    predicate:                 ir-formula,
    schemaVersion:             "1",
    source_kind:               source-kind,
    source_locator:            source-locator
  },
  metadata: {
    ? note: tstr
  }
}
```

#### §1.1.1 Field semantics

| Layer    | Field                       | Required | Meaning |
|----------|-----------------------------|----------|---------|
| envelope | `declaredAt`                | yes      | ISO-8601 UTC minting timestamp. |
| envelope | `signature`                 | yes (swarm) | Ed25519 over `JCS(header ++ metadata)`. |
| envelope | `signer`                    | yes      | `ed25519:<base64>` minter public key. |
| header   | `cid`                       | yes      | Content CID; DERIVED per §3.1. |
| header   | `confidence_basis_points`   | yes      | Lifter's prior on how reliably this source asserts what it claims. 10000 for static-derived (annotations, type signatures); lower for sampled or grammar-extracted predicates (docstrings, empirical witnesses). MUST be in `[0, 10000]`. |
| header   | `extension_fields`          | yes      | Per-kind structured metadata; e.g., `test_target_function_cid` for `test-assertion`-kind evidence pinning the function the assertion targets. MAY be empty `{}`. Keys and values participate in the CID (§3). |
| header   | `kind`                      | yes      | MUST be `"evidence"`. |
| header   | `lifter_cid`                | yes      | CID of the lifter binary or rule-set that emitted this evidence. |
| header   | `predicate`                 | yes      | The asserted predicate (an `IrFormula`). For pre-condition evidence this is the pre; for post-condition evidence this is the post (in pre/post-conjunction form per §6). For an `Option<T>` return-type evidence, this is a predicate of the form `result.is_some() \/ result.is_none()`. |
| header   | `schemaVersion`             | yes      | MUST be `"1"`. |
| header   | `source_kind`               | yes      | One of the canonical labels (§10) or an extension label (unknown labels MUST NOT be rejected by shape validation; downstream consumers decide). |
| header   | `source_locator`            | yes      | Where this evidence was extracted from. |
| metadata | `note`                      | no       | Human-readable annotation. OMITTED when absent. |

**Normative: column counting.** `col` counts UTF-8 BYTES within the line, 0-indexed. Line numbers are 1-indexed. Rationale: bytes are the substrate's native unit; UTF-8 bytes survive transport without re-encoding; converting to codepoints or graphemes requires Unicode-version-specific tables which would roll CIDs as the Unicode standard evolves. Tools that want codepoint-level positions must derive them from the source bytes; the substrate stores bytes. Tab characters (0x09) count as 1 byte, not as tab-stop expansions. CRLF line endings: the CR byte (0x0D) is part of the preceding line; only LF (0x0A) advances the line counter.

### §1.2 `CompoundContractMemento`

```cddl
; A reference to an EvidenceMemento with a per-compound weight (in basis
; points). Under v0 conjunction the weight is informational; under
; best-confidence and loudly-bounded-disjunction (spec'd, not v0) it is
; consulted during verdict derivation.
;
; Locked JCS key order: evidence_cid, weight_basis_points
evidence-ref = {
  evidence_cid:         cid,
  weight_basis_points:  uint            ; 0..10000 inclusive
}

; Open enum; v0 ships ONLY "conjunction". Other strategies have their
; verdict-derivation rule specified in §2, but the Rust impl returns
; Err(WpError::UnimplementedAggregationStrategy) for them (not unimplemented!).
aggregation-strategy = tstr               ; "conjunction" | "best-confidence" | "loudly-bounded-disjunction" | extension label

; The compound-contract memento itself.
;
; Locked JCS key order (header, alphabetical):
;   aggregation_strategy, cid, composed_post, composed_pre, evidences,
;   function_term_cid, kind, schemaVersion
compound-contract-memento = {
  envelope: {
    declaredAt: iso8601,
    signature:  signature,
    signer:     pubkey
  },
  header: {
    aggregation_strategy: aggregation-strategy,
    cid:                  cid,            ; DERIVED -- see §3
    composed_post:        ir-formula,     ; DERIVED -- see §2 and §6
    composed_pre:         ir-formula,     ; DERIVED -- see §2 and §6
    evidences:            [* evidence-ref],
    function_term_cid:    cid,            ; the FunctionContractMemento.cid of the function this is a contract for
    kind:                 "compound-contract",
    schemaVersion:        "1"
  },
  metadata: {
    ? note: tstr
  }
}
```

#### §1.2.1 Field semantics

| Layer    | Field                   | Required | Meaning |
|----------|-------------------------|----------|---------|
| header   | `aggregation_strategy`  | yes      | How per-evidence verdicts compose. v0 normative value: `"conjunction"`. Others spec'd in §2 but unimplemented. |
| header   | `cid`                   | yes      | DERIVED per §3.1. |
| header   | `composed_post`         | yes      | DERIVED. The aggregated post-condition. Under `"conjunction"`, the JCS-normalized conjunction of every evidence's post-predicate (after pre/post separation per §6). Validators MUST recompute and reject on mismatch; see §5.3 INVARIANT (composed-pre/post). |
| header   | `composed_pre`          | yes      | DERIVED. The aggregated pre-condition. Same recompute INVARIANT. |
| header   | `evidences`             | yes      | List of `evidence-ref`s. MAY be empty (degenerate compound; the function has no contract evidence yet; composed_pre/post = `true`/`true` respectively). MUST be sorted by `evidence_cid` ascending at JCS time (§3.2). |
| header   | `function_term_cid`     | yes      | The `FunctionContractMemento.cid` of the function this compound is the contract for. |
| header   | `kind`                  | yes      | MUST be `"compound-contract"`. |
| header   | `schemaVersion`         | yes      | MUST be `"1"`. |

### §1.3 Confidence semantics under conjunction

Under `aggregation_strategy = "conjunction"`, the compound's overall confidence is `min(e.confidence_basis_points for e in evidences)`. The compound is only as confident as its weakest evidence. Rationale: a single low-confidence ambiguous docstring-extracted predicate should not raise the compound's claimed confidence; if the lifter said `5000` for the docstring, the compound is at most 5000-bp confident regardless of how many static-derived 10000-bp annotations sit alongside it. This is the conservative reading and aligns with Supra omnia, rectum.

For `"best-confidence"` (spec'd, not v0): the compound's confidence is `max(e.confidence_basis_points for e in non-refuting evidences)`. For `"loudly-bounded-disjunction"` (spec'd, not v0): the compound's confidence is `max(...)` over the asserting disjuncts.

## §2. The verdict trichotomy at the compound level

Let `E = [e_1, ..., e_n]` be the compound's evidences, and let `v_i` be the per-evidence verdict for `e_i` against the concept's `wp_rule` (computed by the discharger in PR-F).

### §2.1 `aggregation_strategy = "conjunction"` (v0 normative)

The compound's verdict is derived:

```
compound_verdict :=
  if every v_i == "exact"                       then "exact"
  else if any v_i == "refuse"                   then "refuse"
  else                                          "loudly-bounded-lossy"
```

The compound's `loss_record` (carried in the `ConceptSiteMemento.discharge.loss_record` of the binding citing this compound, not in the compound itself) is the per-dimension union of each `loudly-bounded-lossy` evidence's per-evidence loss-record, modulo dataflow (per 2026-05-15 §2.4).

The compound's `composed_pre` is `JCS-normalize(/\_i e_i.predicate_pre)` and `composed_post` is `JCS-normalize(/\_i e_i.predicate_post)`. (See §6 for the pre/post separation rule and the JCS-normalize procedure.)

### §2.2 `aggregation_strategy = "best-confidence"` (spec'd; v0 unimplemented)

The compound's verdict is:

```
non_refuting := [e_i for v_i != "refuse"]
if non_refuting is empty                        then "refuse"
else                                            v_{argmax(e_i.confidence_basis_points, e_i in non_refuting,
                                                          tiebreak by ascending evidence_cid)}
```

Refused evidences contribute to the compound's confidence-score (lowering it) but do not kill the compound if at least one evidence discharges. `composed_pre` and `composed_post` come from the chosen "best" evidence only (NOT from the conjunction); the other evidences contribute to compound-CID-determination by their CIDs but not to the composed pre/post bytes.

### §2.3 `aggregation_strategy = "loudly-bounded-disjunction"` (spec'd; v0 unimplemented)

The compound's verdict is:

```
non_refusing := [e_i for v_i != "refuse"]
if non_refusing is empty                        then "refuse"
else if any v_i == "exact"                      then "exact-with-disjunction-loss"
else                                            "loudly-bounded-lossy"
```

`exact-with-disjunction-loss` is a sub-discriminant of `loudly-bounded-lossy`: the disjunction itself is the loss-record's `structural_divergence` dimension. `composed_pre`/`composed_post` are the JCS-normalized disjunction of evidence predicates. The disjunction structure IS the loss characterization.

### §2.4 Three levels, not two

Three verdict levels in the substrate:

1. **Per-evidence.** The discharger's verdict for `e_i` against the concept's `wp_rule`. Recorded in the discharge receipt (PR-F), not in the evidence.
2. **Compound.** Derived per §2.1 / §2.2 / §2.3 from the per-evidence verdicts.
3. **Binding.** The `ConceptSiteMemento.discharge.verdict`. This IS the compound verdict; the binding cites the compound at `local_contract_cid` and inherits its verdict. There is no further reduction at the binding level beyond what the strategy emitted.

## §3. Content-addressing rules

### §3.1 CID construction

For both mementos, the `cid` is the BLAKE3-512 of the JCS-canonical bytes of the `header` object with `cid` elided.

For `EvidenceMemento`:

```
cid_input = JCS({
  "confidence_basis_points":  <confidence_basis_points>,
  "extension_fields":         <extension_fields>,
  "kind":                     "evidence",
  "lifter_cid":               <lifter_cid>,
  "predicate":                <predicate>,
  "schemaVersion":            "1",
  "source_kind":              <source_kind>,
  "source_locator":           <source_locator>
})
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

For `CompoundContractMemento`:

```
cid_input = JCS({
  "aggregation_strategy": <aggregation_strategy>,
  "composed_post":        <composed_post>,
  "composed_pre":         <composed_pre>,
  "evidences":            <evidences sorted by evidence_cid ascending>,
  "function_term_cid":    <function_term_cid>,
  "kind":                 "compound-contract",
  "schemaVersion":        "1"
})
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

Important properties:

- The `evidences` field of the compound contains evidence-CIDs (not inlined evidence bytes). Changing one evidence's content rolls the evidence's CID, which rolls the compound's CID, which rolls every binding citing that compound.
- `aggregation_strategy` is part of the CID. The same set of evidences aggregated under `"conjunction"` vs `"best-confidence"` produces TWO different compounds (two different CIDs). This is correct: "the conjunction of these evidences" and "the best-confidence pick of these evidences" are different things and live at different addresses.
- `composed_pre` and `composed_post` are part of the CID. They are cached bytes of the derived pre/post; validators MUST recompute and reject on mismatch (§5.3). Cached-with-truth-source means the cache must equal truth, by construction.
- `extension_fields` are JCS-canonicalized per `2026-04-30-canonicalization-grammar.md`. Arbitrary unknown keys roll the CID. This is open-extension under deterministic addressing.

### §3.2 Sub-object canonicalization

Each sub-object is JCS-canonicalized with alphabetical key order. The CDDL above LOCKS that order for human readability; the JCS encoder enforces it normatively.

The compound's `evidences` array MUST be sorted by `evidence_cid` ascending at JCS time. Insertion order is not preserved on the wire. This makes evidence reordering CID-invariant: a Rust value with `evidences = [refB, refA]` and one with `evidences = [refA, refB]` produce the same compound CID after JCS sorting.

NOTE: this crate (`provekit-ir-types`) carries no JCS encoder; round-trip serde tests in this crate verify shape but NOT byte-exact CID stability. CID-stability tests live in `provekit-claim-envelope` (where the JCS encoder lives), per the precedent in `2026-05-12-concept-site-memento.md` §0 and §9.

## §4. Mint procedure

### §4.1 Mint an `EvidenceMemento`

1. Lifter identifies a contract-source in user code (one of the ten kinds in §10).
2. Lifter extracts a predicate (an `IrFormula`) from the source.
3. Lifter records `source_locator` (`source_cid` plus a line/col span).
4. Lifter assigns `confidence_basis_points` per source-kind prior (§10).
5. Lifter fills `extension_fields` with per-kind metadata (see §10).
6. Compute `cid` per §3.1.
7. Sign `JCS({header, metadata})`.

### §4.2 Mint a `CompoundContractMemento` (fresh)

1. Identify the function: obtain `function_term_cid` (the `FunctionContractMemento.cid` of the function this compound is the contract for).
2. Collect all minted `EvidenceMemento`s whose `extension_fields.test_target_function_cid` (or equivalent per-kind back-link, §10) is `function_term_cid`, plus the function's own annotation-derived evidences.
3. Build `evidence-ref`s with `weight_basis_points`. Under `"conjunction"`, MUST be exactly 10000; validators MUST reject any `evidence-ref.weight_basis_points != 10000` when `aggregation_strategy = "conjunction"`. The field is informational under v0 conjunction but is part of the CID, so pinning it normatively ensures CID stability across lifters.
4. Choose `aggregation_strategy` (v0: always `"conjunction"`).
5. Compute `composed_pre` and `composed_post`:
   - For `"conjunction"`: pre/post-separate each evidence's `predicate` (§6), then JCS-normalize-conjunct the pres and the posts.
   - For `"best-confidence"` (spec'd, unimpl): take the highest-confidence non-refuting evidence's separated pre/post.
   - For `"loudly-bounded-disjunction"` (spec'd, unimpl): JCS-normalize-disjunct.
6. Sort `evidences` by `evidence_cid` ascending (the JCS encoder will do this; Rust constructors MAY preserve insertion order).
7. Compute `cid` per §3.1.
8. Sign `JCS({header, metadata})`.

### §4.3 Mint a `CompoundContractMemento` (auto-promotion of bare `FunctionContractMemento`)

**Backward-compat path.** When a consumer encounters a `ConceptSiteMemento.local_contract_cid` that resolves to a bare `FunctionContractMemento`, the validator auto-promotes it to a single-evidence compound on the fly:

1. Mint one `EvidenceMemento` with:
   - `source_kind = "annotation"`.
   - `predicate` = the `FunctionContractMemento`'s `pre /\ post` packaged per §6.
   - `confidence_basis_points = 10000`.
   - `source_locator` = derived from the `FunctionContractMemento`'s `locus`.
   - `lifter_cid` = `"blake3-512:0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"` (128 hex `0`s; a reserved sentinel CID, §4.4).
   - `extension_fields = { "auto_promoted_from": <function_contract_cid> }`.
2. Build a `CompoundContractMemento` with `evidences = [refToTheEvidence]` where the evidence-ref has `weight_basis_points = 10000` (normative per §4.2 step 3), `aggregation_strategy = "conjunction"`, `composed_pre`/`composed_post` from the bare contract.
3. The promoted compound's CID is recomputed per §3.1.

The auto-promoted compound has a fresh CID different from the bare contract's CID. The substrate stores this mapping (bare-contract-CID -> promoted-compound-CID) in the pool index so future lookups bypass re-promotion.

### §4.4 The sentinel `auto-promote` lifter CID

A reserved CID `blake3-512:0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000` (128 hex `0`s) identifies the auto-promotion path as the lifter. Downstream consumers can detect auto-promoted compounds by checking this lifter CID. The sentinel CID is NOT a valid BLAKE3-512 hash of any real lifter binary; it is reserved for this purpose by spec. The probability that a real BLAKE3-512 digest equals all-zeros is 2^-512; the sentinel is provably distinct from any real hash output.

Pass-1 validation (§5.1) accepts this sentinel at `lifter_cid`: "128-hex" is satisfied by 128 hex `0` digits. No special-case exception to the CID format rule is needed.

**INVARIANT (mint idempotency):** Two mint operations with byte-identical inputs MUST produce the same `cid` for both kinds.

### §4.5 Byte-offset to line/col conversion (normative)

When auto-promotion (§4.3) derives a `source_locator` from a `FunctionContractMemento.locus` (which uses byte-offset spans), the byte-offset MUST be converted to line/col using the following algorithm to ensure CID determinism across implementations:

```
Input:  source_bytes (UTF-8 encoded source file contents as a byte array)
        byte_offset (usize, 0-indexed byte position within source_bytes)
Output: { line: u32, col: u32 }

Algorithm:
  line := 1
  col  := 0
  for i in 0..byte_offset:
    if source_bytes[i] == 0x0A (LF):
      line := line + 1
      col  := 0
    else:
      col  := col + 1
  return { line, col }
```

Properties and constraints:

- `line` is 1-indexed (starts at 1 for offset 0).
- `col` is 0-indexed UTF-8 byte count from the most recent LF (or start of file) to `byte_offset`.
- CRLF line endings: CR (0x0D) increments `col` as an ordinary byte; only LF (0x0A) resets `col` and increments `line`. This means the CR byte is counted as part of the preceding line's col.
- `byte_offset` MUST point at a UTF-8 character boundary. If it does not, the conversion result is unspecified and validators MUST reject the containing `EvidenceMemento`.
- `byte_offset` MUST be in `0..=source_bytes.len()`. An offset equal to `source_bytes.len()` is the one-past-the-end position (end of last line).
- Tab characters (0x09) count as 1 byte in the `col` counter (no tab-stop expansion).

This algorithm is byte-deterministic: two implementations fed identical `source_bytes` and `byte_offset` MUST produce identical `{ line, col }` outputs, which produces identical `source_locator` bytes, which produces an identical `EvidenceMemento` CID.

## §5. Validation rules

### §5.1 Pass 1: CDDL shape check (both kinds)

Reject if:

- Any required field is missing.
- For `EvidenceMemento`: `kind != "evidence"`, `schemaVersion != "1"`, `confidence_basis_points > 10000`.
- For `CompoundContractMemento`: `kind != "compound-contract"`, `schemaVersion != "1"`, any `evidence-ref.weight_basis_points > 10000`, or (when `aggregation_strategy = "conjunction"`) any `evidence-ref.weight_basis_points != 10000`.
- Any hash/CID field does not match `"blake3-512:" ++ 128-hex`, EXCEPT in the slots admitting the `pending:<symbol>` sentinel form per §10.1.
- For `CompoundContractMemento`: `aggregation_strategy` is not one of the canonical labels AND v0 does not accept extension labels at all (per §0; only `"conjunction"` is wired in v0; the others are spec'd but return `Err(WpError::UnimplementedAggregationStrategy)` in Rust).

NOTE on `source_kind`: validators MUST accept unknown source-kind labels at shape level (open extension). Downstream consumers decide whether to refuse unknown kinds; the spec does not.

### §5.2 Pass 2: degenerate-compound check

If `evidences` is empty:

- `composed_pre` MUST equal the `IrFormula` representation of `true`.
- `composed_post` MUST equal the `IrFormula` representation of `true`.
- `aggregation_strategy` is informational (does not apply); SHOULD be `"conjunction"` for least-surprise.

The compound verdict for an empty-evidences compound is `"exact"`. By §2.1: "if every v_i == exact, then exact"; with zero evidences the condition holds vacuously. Downstream consumers MUST NOT second-guess this: an empty-evidences compound has exactly the trivial contract `{ pre: true, post: true }` with verdict `"exact"` (trivially satisfied).

Rationale: a function with no extracted evidence has the trivial contract `{ pre: true, post: true }`. This is the substrate's degenerate base case and is valid; it is the starting point before any lifter runs.

### §5.3 Pass 3: DERIVED constraints

**INVARIANT (cid):** Recompute `cid` per §3.1 and verify it equals `header.cid`. Reject on mismatch (both kinds).

**INVARIANT (composed-pre/post):** For `CompoundContractMemento` under `aggregation_strategy = "conjunction"`:

- Resolve each `evidence-ref.evidence_cid` against the pool to fetch the `predicate`.
- Pre/post-separate each predicate per §6.
- JCS-normalize-conjunct the pres and the posts.
- Verify that the result equals `header.composed_pre` / `header.composed_post` byte-for-byte under JCS. Reject on mismatch.

Under `"best-confidence"` and `"loudly-bounded-disjunction"`, parallel INVARIANTs hold per §2.2 and §2.3; these are spec'd but unimplemented in v0.

**INVARIANT (sort):** The `evidences` array MUST be sorted by `evidence_cid` ascending in JCS-canonical bytes. Validators MUST reject on out-of-order arrays (the JCS encoder would sort, so any wire-shape that isn't sorted is malformed; reject without sorting).

**SIGNATURE:** For swarm-distributed mementos, verify `envelope.signature` over `JCS({header, metadata})` against `envelope.signer`. Reject on invalid signature.

### §5.4 Pass 4: REFERENT constraints (pool-level)

For `EvidenceMemento`:

- The pool MUST contain a canonical-source artifact with `cid = header.source_locator.source_cid`.
- The pool MUST contain a lifter binary or rule-set with `cid = header.lifter_cid` (EXCEPT when `lifter_cid` is the reserved `auto-promote` sentinel, §4.4).
- Any extension-field CIDs (e.g., `test_target_function_cid`) MUST resolve to existing mementos.

For `CompoundContractMemento`:

- The pool MUST contain a `FunctionContractMemento` with `cid = header.function_term_cid`.
- For each `evidence-ref` in `evidences`, the pool MUST contain an `EvidenceMemento` with that CID.

## §6. Composition semantics

### §6.1 Pre/post separation

An `EvidenceMemento.predicate` is an `IrFormula`. Some sources naturally produce pre-conditions (`#[requires]`, `assert!(b != 0)` at function start), some produce post-conditions (`#[ensures]`, return-type predicates, test assertions like `assert_eq!(f(x), y)`), and some produce a coupled pair (e.g., a docstring "Returns None if key missing" couples a pre-condition on the input with a post-condition on the result).

The spec defines a pre/post separation rule on `IrFormula`:

- A formula of shape `requires(P)` separates to `(pre = P, post = true)`.
- A formula of shape `ensures(Q)` separates to `(pre = true, post = Q)`.
- A formula of shape `requires(P) /\ ensures(Q)` separates to `(pre = P, post = Q)`.
- A formula not matching any of these defaults to `(pre = true, post = predicate)` (treated as a pure post-condition).

Rationale: every contract-source ultimately reduces to `requires(P) -> ensures(Q)` semantics; the separation rule normalizes the rep so the compound can JCS-conjunct cleanly. The grammar of `requires` / `ensures` markers in `IrFormula` follows `2026-04-30-ir-formal-grammar.md`.

### §6.2 JCS-normalize-conjunct

`JCS-normalize-conjunct(P_1, ..., P_n)` is defined as:

1. De-duplicate at the predicate-CID level: for each `P_i`, its predicate-CID is `"blake3-512:" ++ hex(BLAKE3-512(JCS(P_i)))`. `P_i` and `P_j` with identical predicate-CIDs (i.e., identical JCS bytes) collapse to one occurrence.
2. Sort by predicate-CID ascending.
3. Build the conjunction term `and(P_1, ..., P_n)` in `IrFormula`.
4. JCS-canonicalize.

This makes the composed predicate's bytes a function of the unordered multi-set of unique evidence predicates, not of insertion order.

### §6.3 Discharge composition (PR-F preview)

When a `ConceptSiteMemento` cites a `CompoundContractMemento` at `local_contract_cid`, the discharger:

1. Pulls the compound.
2. For each `evidence-ref`, pulls the `EvidenceMemento`, computes its per-evidence verdict against the concept's `wp_rule`.
3. Derives the compound verdict per the recorded `aggregation_strategy` (§2).
4. Records per-evidence verdicts in the `MorphismDischargeReceipt` (per 2026-05-15 §2.5).
5. Sets the binding's `discharge.verdict` to the compound verdict.

The receipt is the level where per-evidence verdicts are durably content-addressed. The compound is the level where the AGGREGATION is durably content-addressed. The binding is the level where the SITE-CONCEPT relation is durably content-addressed. Three mementos, three layers, one chain.

## §7. Worked example

A user's Rust function `fn safe_div(a: i32, b: i32) -> Option<i32>` with:

1. `#[requires(b != 0)]` annotation.
2. `#[ensures(result.is_some() iff b != 0)]` annotation.
3. A test `#[test] fn t1() { assert_eq!(safe_div(10, 2), Some(5)); }`.
4. A test `#[test] fn t2() { assert_eq!(safe_div(10, 0), None); }`.
5. A docstring `/// Returns None when the divisor is zero.`
6. The `Option<i32>` return type.
7. The `assert!(b != 0)` inline at function start.

The substrate lifts each into an `EvidenceMemento`:

| # | `source_kind`         | `predicate` (sketch)                                | `confidence_basis_points` | extension                                              |
|---|-----------------------|-----------------------------------------------------|---------------------------|--------------------------------------------------------|
| 1 | `annotation`          | `requires(b != 0)`                                  | 10000                     | `{}`                                                   |
| 2 | `annotation`          | `ensures(result.is_some() iff b != 0)`              | 10000                     | `{}`                                                   |
| 3 | `test-assertion`      | `ensures(b == 2 /\ a == 10 -> result == Some(5))`   | 10000                     | `{ "test_target_function_cid": <safe_div_cid> }`       |
| 4 | `test-assertion`      | `ensures(b == 0 /\ a == 10 -> result == None)`      | 10000                     | `{ "test_target_function_cid": <safe_div_cid> }`       |
| 5 | `docstring`           | `ensures(b == 0 -> result == None)`                 | 6500                      | `{ "extracted_phrase": "Returns None when..." }`       |
| 6 | `type-signature`      | `ensures(result.is_some() \/ result.is_none())`     | 10000                     | `{ "return_type": "Option<i32>" }`                     |
| 7 | `implicit-effect`     | `requires(b != 0)`                                  | 10000                     | `{ "assert_site_line": 7 }`                            |

Six unique evidence-mementos (1 and 7 have byte-identical predicates -- both `requires(b != 0)` -- but DIFFERENT `extension_fields` and DIFFERENT `source_locator`s, so they have different CIDs and are NOT de-duplicated at the evidence level; they ARE de-duplicated at the JCS-normalize-conjunct level in §6.2 because their `predicate` bytes are identical after canonicalization).

The compound is built under `"conjunction"`. Sorted-by-CID, the seven evidence-refs assemble. The `composed_pre` after §6.2 normalization is `b != 0` (de-duplicated from #1 and #7). The `composed_post` is the conjunction of the five post-side predicates (de-duplicated where bytes coincide).

The binding (in the `ConceptSiteMemento`) cites this compound at `local_contract_cid` and binds to a catalog concept (likely `concept:partial-function-by-guard` or `concept:option-from-guard`). The discharger:

- Per-evidence verdict on #1: `exact` against the concept's pre.
- Per-evidence verdict on #2: `exact`.
- Per-evidence verdicts on #3, #4: `exact` (witnesses on the algebraic post).
- Per-evidence verdict on #5: `loudly-bounded-lossy` (the docstring claim is strict-subset of the algebraic post; loss in `value_divergence` characterizes "what the docstring doesn't say").
- Per-evidence verdict on #6: `loudly-bounded-lossy` (the type-signature only pins disjointness, not the iff structure).
- Per-evidence verdict on #7: `exact` (matches #1's pre exactly).

Compound verdict under `"conjunction"`: `loudly-bounded-lossy` (because #5 and #6 are loudly-bounded-lossy; no refuse). The receipt records all seven per-evidence verdicts plus the compound verdict plus the union of per-evidence loss-records.

The binding's `discharge.verdict` = `loudly-bounded-lossy`. Composition through this binding propagates the union loss-record. A future re-lift of the docstring with a tighter grammar produces a new evidence-memento (different CID), which produces a new compound (different CID), which produces a new binding (different CID) with a tighter loss-record. The old binding and the new binding coexist in the pool. Different addresses, different things.

## §8. Roadmap (PR-A through PR-H)

This PR-A lands the SPEC and the Rust types only.

- **PR-A (this PR):** CDDL spec at `protocol/specs/2026-05-13-compound-contract-memento.md` (this document) and `EvidenceMemento`, `CompoundContractMemento`, `EvidenceRef`, `SourceKind`, `AggregationStrategy`, `SourceLocator`, `SourceLocatorSpan`, `SourceLocatorPoint` types in `provekit-ir-types/src/lib.rs` with serde round-trip tests.
- **PR-B (backward-compat lift):** Auto-promotion. The validator path that encounters a bare `FunctionContractMemento` at `ConceptSiteMemento.local_contract_cid` mints a single-evidence compound on the fly (§4.3). The promotion is cached pool-side so subsequent lookups are O(1).
- **PR-C (per-source lifter: test assertions):** Walks `#[test]` functions; for each `assert_eq!(f(...), expected)`, emits an `EvidenceMemento` with `source_kind = "test-assertion"` whose `extension_fields.test_target_function_cid` pins the lifted function-CID of `f`. Re-mints the function's compound to include the new evidences.
- **PR-D (per-source lifter: type signatures):** Reads the function's signature; generates partial-post evidences from return types (`-> Option<T>` produces `result.is_some() \/ result.is_none()`; `-> Result<T, E>` produces a disjointness predicate; `-> Vec<T>` produces `result.len() >= 0`, and so on).
- **PR-E (per-source lifter: docstrings):** Extracts `/// Returns ... if ...` patterns with a small grammar. Conservative on ambiguity (emits `confidence_basis_points < 10000`).
- **PR-F (compound-aware discharge):** `libprovekit::wp` discharger consumes a `CompoundContractMemento` and discharges each evidence; derives the compound verdict per the recorded `aggregation_strategy`. Mints a `MorphismDischargeReceipt` (per 2026-05-15 §2.5) that records per-evidence verdicts. This is also where v0 cuts over the `ConceptSiteMemento` mint path to point `local_contract_cid` at compounds (was: pointed at bare contracts).
- **PR-G (native contract surfaces):** Per-language lifters for JML, Zod, Spring annotations, pydantic, and OpenAPI. Each emits `source_kind = "native-surface"` evidence with the canonical surface name in `extension_fields.surface_name`.
- **PR-H (smoke-test demonstration):** `menagerie/smoke-test-e2e/` lifts multiple evidences per fixture function. `report.md §11` shows per-evidence + compound verdicts for at least the `safe_div` exemplar from §7.

## §9. Smoke test (the acceptance test for the compound layer)

A complete round-trip on a real Rust function with multiple evidence sources:

1. Lift the function once with `provekit-lift-contracts`: produces a `FunctionContractMemento` for source (1)-(2) plus a list of evidence-mementos for (1)-(7).
2. Build the `CompoundContractMemento` aggregating all evidences under `"conjunction"`.
3. Bind the function to a catalog concept; mint a `ConceptSiteMemento` with `local_contract_cid` pointing at the compound.
4. Discharge: verify per-evidence verdicts are recorded in the receipt; verify the compound verdict is derived correctly; verify the binding verdict equals the compound verdict.

**Acceptance for PR-A (this PR):**

- `cargo test -p provekit-ir-types` is green on the compound serde round-trip tests.
- `cargo check --workspace` is clean.

**Acceptance for PR-F (full end-to-end):**

- The compound's `evidences.len() >= 3` for the exemplar function.
- The compound's `composed_pre` is the conjunction of evidence pres.
- The discharge runs against the concept's `wp_rule` and records per-evidence verdicts in the receipt.
- The byte-exact CID stability tests for the compound live in `provekit-claim-envelope` (where the JCS encoder lives), NOT in `provekit-ir-types`.

## §10. Per-source lifter inventory

The ten canonical `source_kind` labels and their extraction guidance:

| `source_kind`              | Extracted by                                   | `confidence_basis_points` prior | Required `extension_fields`                                            |
|----------------------------|------------------------------------------------|---------------------------------|------------------------------------------------------------------------|
| `annotation`               | `provekit-lift-contracts` (existing)           | 10000                           | `{}` (or `{ "auto_promoted_from": <fcm_cid> }` for backward-compat path) |
| `test-assertion`           | PR-C (new walker)                              | 10000                           | `{ "test_target_function_cid": <cid> }`                                |
| `type-signature`           | PR-D (signature reader)                        | 10000                           | `{ "return_type": <type_string> }`                                     |
| `docstring`                | PR-E (grammar-based extractor)                 | 5000-8000 (per grammar match)   | `{ "extracted_phrase": <text> }`                                       |
| `loop-invariant`           | existing `LoopInvariantMemento` lifter         | 10000                           | `{ "loop_invariant_memento_cid": <cid> }`                              |
| `implicit-effect`          | walker over `assert!` / `panic!` / `unwrap` / `?` call-sites | 10000           | `{ "effect_site_line": <uint>, "effect_kind": "assert" \| "panic" \| "unwrap" \| "try" }` |
| `native-surface`           | PR-G (per-language: JML, Zod, Spring, pydantic, OpenAPI) | 10000           | `{ "surface_name": "jml" \| "zod" \| "spring" \| "pydantic" \| "openapi" }` |
| `structural-synthesis`     | clustering-mint time per `2026-05-13-wp-as-formula.md` | 10000           | `{ "synthesized_from_cluster_cid": <cid> }`                            |
| `empirical-witness`        | future witness floor (PR-F's witness sampler)  | 1-9999 (sample-confidence-driven) | `{ "witness_memento_cid": <cid>, "sample_count": <uint> }`           |
| `review-comment`           | future (out of scope for v0; placeholder)      | 1000-5000                       | `{ "pr_url": <string>, "comment_id": <string> }`                       |

Each label is extension-open: validators MUST accept unknown labels at shape level (§5.1). Downstream consumers MAY refuse to compose through an unknown-kind evidence.

Every `extension_fields` slot in this table that names a CID-valued back-link (`test_target_function_cid`, `loop_invariant_memento_cid`, `witness_memento_cid`, `synthesized_from_cluster_cid`) MAY carry the `pending:<symbol>` sentinel form per §10.1 in place of a resolved BLAKE3-512 CID, with the validation carve-out specified there. The `lifter_cid` of an `EvidenceMemento` (and the `source_locator.source_cid`) MUST NOT carry the sentinel; those are resolved at mint by construction.

### §10.1 The `pending:<symbol>` sentinel (loudly-bounded-lossy form)

**Normative.** A CID-valued field MAY carry the literal string form `pending:<symbol>` in place of a `blake3-512:` digest in the slots enumerated in §10.1.2, and only there. The sentinel is a content-addressed declaration that the field's resolved target is known by name but not yet resolved to a CID (cross-crate links, forward references, lazy resolution during multi-pass mint). The sentinel's wire-form existence IS the binding's contract: the substrate openly records that the target is unresolved rather than silently substituting a placeholder digest. This is the loudly-bounded-lossy arm of the trichotomy (per `docs/papers/09-lossy-boundary-compression.md` and the 2026-05-11 Supra omnia rectum refinement: never claim more than you can prove; an openly-recorded placeholder plus a characterized loss is legitimate, a silent placeholder is forbidden).

#### §10.1.1 Syntax

```
pending-sentinel = "pending:" symbol
symbol           = symbol-head symbol-tail*
symbol-head      = ALPHA / "_"
symbol-tail      = ALPHA / DIGIT / "_" / ":" / "."
```

Where `ALPHA` is `[A-Za-z]` and `DIGIT` is `[0-9]`. Both `::` (Rust / C++ qualified path) and `.` (Java / Python qualified path) are permitted in the symbol body to reach cross-language naming conventions. The symbol MUST NOT contain whitespace and MUST NOT be empty.

Validators MUST reject (Pass 1, §5.1) the malformed forms:

- `pending:` (no symbol body).
- The bare word `pending` (no colon).
- Any `pending:<symbol>` containing ASCII whitespace, control characters, or any character outside the grammar above.
- The literal `pending::` (no name after the leading separator) or `pending:.` (no name after the leading separator).

A well-formed `pending:<symbol>` is treated by the JCS canonicalizer (`2026-04-30-canonicalization-grammar.md`) as a UTF-8 string literal in the field's position. No CDDL change is required beyond a regex-validated `tstr` at the admitting slots; the open-extension nature of `tstr` accommodates the sentinel form without grammar amendment.

#### §10.1.2 Permitted slots

The sentinel form MAY occupy the following slots in the substrate:

| Memento | Field | Rationale |
|---|---|---|
| `EvidenceMemento` | `extension_fields.test_target_function_cid` | The test assertion's target function may not yet be lifted into the pool (cross-crate test). |
| `EvidenceMemento` | `extension_fields.loop_invariant_memento_cid` | A loop invariant evidence may be minted before its referent `LoopInvariantMemento` has been content-addressed. |
| `EvidenceMemento` | `extension_fields.witness_memento_cid` | A witness reference may be minted ahead of the witness's discharge cycle. |
| `EvidenceMemento` | `extension_fields.synthesized_from_cluster_cid` | A structural-synthesis evidence may reference a cluster CID resolved in a later mint pass. |
| `CompoundContractMemento` | `header.function_term_cid` | The compound may be minted before its target `FunctionContractMemento` is resolved (cross-crate compound, two-pass lift). |

The following slots MUST NOT carry the sentinel; validators MUST reject any `pending:<symbol>` at these positions in Pass 1 (§5.1):

| Memento | Field | Rationale |
|---|---|---|
| `EvidenceMemento` | `header.lifter_cid` | The lifter is required to be resolved at mint time (the evidence's provenance is load-bearing); the §4.4 all-zeros sentinel is a distinct CID-shape mechanism for the auto-promotion lifter and is NOT equivalent to `pending:<symbol>`. |
| `EvidenceMemento` | `header.source_locator.source_cid` | The source artifact is the lift input; it MUST be content-addressed before the evidence is minted. |
| `EvidenceMemento.header.cid` | (the memento's own CID) | A memento's own CID is DERIVED per §3.1 from its other bytes; it cannot be pending. |
| `CompoundContractMemento.header.cid` | (the memento's own CID) | Same as above. |
| `CompoundContractMemento` | `evidences[*].evidence_cid` | The compound's §5.3 INVARIANT (composed-pre/post) requires resolving each evidence's predicate to recompute `composed_pre`/`composed_post` byte-for-byte. A pending evidence_cid makes that recompute undefined and breaks the cached-with-truth-source duality. |

#### §10.1.3 Loss-record contribution and trichotomy

Per `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3, the `loss-record` schema is `{ loss-dimension => ir-formula }` with `loss-dimension` an open `tstr` enum. This spec extends that open enum with:

```
loss-dimension /= "pending-target-not-resolved"
```

When the discharger (PR-F) encounters a `pending:<symbol>` sentinel at any admitted slot during compound discharge, it MUST:

1. Emit a per-evidence verdict of `loudly-bounded-lossy` for the affected evidence (or `loudly-bounded-lossy` at the compound level when the sentinel sits on `CompoundContractMemento.function_term_cid`).
2. Contribute the formula `pending_target(<symbol>)` (an `IrFormula` literal naming the unresolved symbol) to the discharge receipt's `loss_record` under the `pending-target-not-resolved` dimension. If multiple sentinels are present, the dimension's formula is the JCS-normalized conjunction of one `pending_target(<symbol_i>)` per occurrence.
3. NEVER discharge the receipt as `"exact"` against a compound carrying any `pending:<symbol>` target in an admitted slot. The discharger MUST refuse to claim resolution it cannot prove. A refusal here is loudly-bounded-lossy with a characterized loss, NOT `"refuse"`: the loss is precisely "the target named `<symbol>` is not yet resolved," and that IS the contract.

At the compound verdict level under `aggregation_strategy = "conjunction"` (§2.1), any per-evidence `loudly-bounded-lossy` arising from a pending sentinel propagates to the compound verdict per the existing §2.1 rule. The binding's `ConceptSiteMemento.discharge.verdict` inherits this per §2.4 (three levels). The `DomainClaim` wire form (`2026-05-13-domain-claim-normalization.md` §1.2) records the `loudly-bounded-lossy` verdict with the `pending-target-not-resolved` loss dimension populated; the verdict-consistency invariant (§1.2 of that spec: `loudly-bounded-lossy` REQUIRES non-empty `loss_record`) is satisfied by construction.

#### §10.1.4 Resolution semantics

At any later mint pass, if the symbol resolves to a real CID:

1. The minter constructs a NEW `EvidenceMemento` (or `CompoundContractMemento`) with the resolved CID in place of the sentinel.
2. The new memento's `cid` (§3.1) differs from the pending memento's `cid` by construction: the field is part of the JCS-canonical bytes, so substituting `pending:<symbol>` with a real CID rolls the memento's CID.
3. The OLD pending memento and the NEW resolved memento are independent content-addresses; they ARE NOT "the same memento updated." Content-addressing forbids mutation (§3.1 INVARIANT mint-idempotency).
4. Convention: mint the resolved one as a successor. The pending memento stays in the trail. Bindings that cited the pending compound continue to cite the pending compound; bindings minted after resolution cite the resolved compound. Both addresses coexist in the pool.

There is no spec-defined back-link from the resolved memento to the pending memento. Tooling MAY record the succession in an external index (out of scope for this amendment); the substrate's content-addressing makes any such index reconstructible by symbol lookup.

#### §10.1.5 Distinction from the §4.4 auto-promote sentinel

The §4.4 reserved all-zeros lifter CID (`blake3-512:` followed by 128 hex `0`s) is a distinct mechanism: it is a well-formed BLAKE3-512 CID-shape that satisfies the Pass 1 (§5.1) "128-hex" rule by construction and identifies the auto-promotion lifter. It occupies the `lifter_cid` slot, which §10.1.2 forbids to `pending:<symbol>`. Implementations MUST NOT conflate the two: the all-zeros sentinel is a resolved-but-reserved CID; the `pending:<symbol>` sentinel is an openly-recorded unresolved-target string.

#### §10.1.6 Worked example

A multi-crate Rust workspace where crate `accounts` contains a function `transfer` that calls into crate `ledger`. A test in crate `accounts` asserts on `transfer`'s behavior; the lifter for `accounts` runs before the lifter for `ledger`. The test-assertion evidence is minted with a pending target for the cross-crate function reference:

```json
{
  "envelope": {
    "declaredAt": "2026-05-13T15:21:09Z",
    "signature":  "ed25519:...",
    "signer":     "ed25519:..."
  },
  "header": {
    "cid":                     "blake3-512:7b4c...d9a3",
    "confidence_basis_points": 10000,
    "extension_fields": {
      "test_target_function_cid": "pending:account::transfer"
    },
    "kind":          "evidence",
    "lifter_cid":    "blake3-512:8e21...4f0b",
    "predicate":     { "ensures": { "...": "..." } },
    "schemaVersion": "1",
    "source_kind":   "test-assertion",
    "source_locator": {
      "source_cid": "blake3-512:1a5f...c2e7",
      "span":       { "end": { "line": 18, "col": 1 }, "start": { "line": 14, "col": 4 } }
    }
  },
  "metadata": {}
}
```

When this evidence is rolled into a compound for the (later-resolved) `account::transfer` function, the discharger's receipt records (sketch):

```json
{
  "per_evidence_verdicts": [
    {
      "evidence_cid": "blake3-512:7b4c...d9a3",
      "verdict":      "loudly-bounded-lossy",
      "loss_record": {
        "pending-target-not-resolved": "pending_target(account::transfer)"
      }
    }
  ],
  "compound_verdict": "loudly-bounded-lossy",
  "compound_loss_record": {
    "pending-target-not-resolved": "pending_target(account::transfer)"
  }
}
```

The binding's `DomainClaim` (per `2026-05-13-domain-claim-normalization.md`) carries `verdict.kind = "loudly-bounded-lossy"` with `loss_record = { "pending-target-not-resolved": "pending_target(account::transfer)" }`. The verdict-consistency invariant (that spec §1.2: loudly-bounded-lossy REQUIRES non-empty loss_record) holds.

When the `ledger` lifter runs and `account::transfer` resolves to a real `FunctionContractMemento` at CID `blake3-512:f0c2...92ab`, a successor evidence is minted with `extension_fields.test_target_function_cid = "blake3-512:f0c2...92ab"`. Its CID differs from the pending evidence's CID (the extension_fields bytes differ). The successor flows into a successor compound; a fresh binding minted against the resolved compound discharges to `"exact"` (no pending loss). The pending memento and the resolved memento both persist; different addresses, different things.

## §11. Cross-references

- This spec AMENDS `2026-05-12-concept-site-memento.md` §1.1 and §5.4 (§0.4 above).
- The `function_term_cid` of `CompoundContractMemento` is the `FunctionContractMemento.cid` per `2026-05-03-contract-cid-vs-attestation-cid.md` (contract CID, not attestation CID).
- The `predicate` field of `EvidenceMemento` is an `IrFormula` per `2026-04-30-ir-formal-grammar.md`.
- JCS canonicalization per `2026-04-30-canonicalization-grammar.md`.
- Structural-synthesis evidences (`source_kind = "structural-synthesis"`) flow from `2026-05-13-wp-as-formula.md`.
- The per-evidence verdict trichotomy IS the §2 trichotomy of `2026-05-12-concept-site-memento.md`; this spec adds the compound-level derivation rule on top.
- Loss-record dimensions per `2026-05-15 §2.4`.
- §10.1 extends the open `loss-dimension` enum of `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3 with `"pending-target-not-resolved"`.
- §10.1 trichotomy alignment with `2026-05-13-domain-claim-normalization.md` §1.2 (verdict-consistency invariant: `loudly-bounded-lossy` REQUIRES non-empty `loss_record`; the pending-sentinel discharge satisfies this by construction).

## §12. Out of scope for PR-A

- Implementation of the discharger that consumes compounds (PR-F).
- Implementation of any per-source lifter beyond the `annotation` path that already exists (PR-B through PR-G).
- The smoke-test demonstration (PR-H).
- Byte-exact CID-pinning tests for the compound (live in `provekit-claim-envelope`).
- Wire-level migration of existing `ConceptSiteMemento`s in any deployed pool (handled as a one-shot pool-walker in PR-B).
- `"best-confidence"` and `"loudly-bounded-disjunction"` aggregation behavior (spec'd in §2.2 / §2.3; v0 Rust returns `Err(WpError::UnimplementedAggregationStrategy)` for them).

PR-A is the SPEC, the Rust TYPES, and the serde round-trip tests. Validation passes 1-2 are testable from the types layer (CDDL-shape + degenerate-compound); pass 3 (DERIVED constraints) requires the JCS encoder and is tested in `provekit-claim-envelope`; pass 4 (pool REFERENT) is tested in PR-B when the pool is wired.
