# Concept-Citation Comment Sugar (`pep/1.8.0`, liftable operation-identity comments)

**Status:** v1.0.0 normative draft.
**Date:** 2026-05-15
**Author:** T Savo
**Related:**
- `2026-05-14-contract-comment-sugar.md` (sibling sugar surface; carries contract clauses, not operation identity)
- `2026-05-12-sugar-dict-memento.md` (`kind = "sugar"`, selection and loss scoring)
- `2026-05-13-compound-contract-memento.md` (`EvidenceMemento`, `CompoundContractMemento`)
- `2026-05-13-body-template-memento.md` (sibling: executable body cells)
- `2026-05-14-transport-gap-and-partial-morphism-protocol.md` §1.3 (`loss-record`)
- `2026-04-30-canonicalization-grammar.md` (JCS + BLAKE3-512)
- `2026-04-30-ir-formal-grammar.md` (term tree position convention)
- Issues #966 (concept tags + human shorthand), #934 (contract-comment-sugar pattern), #756 (concept tags as audit and relift anchor), #965 (generic programmer comments, not substrate), #1018 (missing-template refusal receipt + `term_position` convention), #1022 (next consumer of this carrier).

## §0. Purpose

The universal comment floor in `2026-05-12-sugar-dict-memento.md` can always
emit human-readable prose, but prose alone is not a substrate fact about which
operation occurred. Contract-comment sugar (`2026-05-14-contract-comment-sugar.md`)
solves the parallel problem for FOL/contract clauses. There is still no
liftable carrier for **operation identity** when a transported operation has
no native target surface:

- `drop(x)` in Rust travels into Python where the host runtime garbage-collects
  and offers no surface that names the drop.
- `free(ptr)` in C travels into Java or Python where memory is managed and the
  host offers no surface that names the free.
- A target-side `call` or `method` whose substrate operation differs from any
  syntactic invocation the host can express (e.g. an effectful operation whose
  effect handler was discharged at lift time but whose identity must still be
  carried for the next lift).

Without a CID-backed carrier, the next lift sees only host syntax and cannot
recover the substrate operation. The transport silently loses operation
identity. That is incompatible with the substrate's correctness rule.

This spec defines the liftable concept-citation comment surface. A round trip
needs:

1. the realizer emits a source-language comment that carries a canonical
   concept-citation payload at the position the operation would have occupied;
2. a human or tool can inspect the payload as an audit trail;
3. the lifter can recover the payload as a substrate operation whose identity
   equals the cited `concept_cid`; and
4. the recovered operation cites the same shape CID, args CID, and site CID, or
   the lifter refuses.

This is a sugar surface, not a new memento family. A conforming implementation
still mints/consumes `EvidenceMemento`, `SugarDictMemento`, `LossRecordMemento`,
and the existing concept-shape catalog. The comment is the carrier. The CID is
the name.

### §0.1 Sugar surface comparison

| Surface | Liftable | Loss expectation | Purpose | `artifact_kind` |
|---|---:|---|---|---|
| prose comment floor (#965) | no | non-empty `machine_uncheckable_prose` | Human breadcrumb when no rigorous surface is selected. Never substrate. | n/a |
| contract-comment sugar | yes | empty only when the canonical formula payload relifts byte-identically | Machine-readable FOL/contract clause evidence embedded in host comments. | `provekit-contract-comment-sugar` |
| concept-citation sugar | yes | empty only when concept/shape/args CIDs and payload CID all relift byte-identically | Machine-readable **operation identity** embedded in host comments when no native target surface exists. | `provekit-concept-citation-comment-sugar` |

A realizer MUST NOT claim the prose floor is exact. A contract-comment payload
identifies a clause, not an operation. A concept-citation payload identifies an
operation, not a clause. The two carriers do not substitute for each other, and
§7 forbids mixing them on the same logical operation.

### §0.2 CID authority

Human-readable fields (`concept_name`, any presentation shorthand from #966) are
diagnostic and editable. They do not identify the substrate operation. The
authoritative pointers are CIDs:

- `concept_cid` points to the concept-shape memento that names the operation.
- `shape_cid` points to the operation-shape spec from
  `menagerie/concept-shapes/specs` that the concept resolves to.
- `args_jcs_cid` points to the canonical bytes of the argument list.
- `concept_site_cid` and (optional) `callsite_cid` point to the originating
  site and invocation, respectively.
- `loss_record_cid`, `sugar_dict_cid`, and `policy_cid` point to the selection
  and loss facts that explain why this surface exists.

If a CID field and a human-readable field disagree, the CID wins. If a CID
cannot be resolved or recomputed, the lifter MUST fail closed (§6).

## §1. Wire shape

The embedded payload is a JCS-canonical JSON object. Hosts MAY wrap it in any
language-idiomatic comment syntax (§3), but the payload bytes inside the wrapper
MUST be recoverable as UTF-8 JSON.

```cddl
; Imports:
;   cid           ; "blake3-512:" 128HEXDIG
;   loss-record   ; from 2026-05-14-transport-gap-and-partial-morphism-protocol.md §1.3
;   term-position ; sequence of uint, the path of integer indices into the
;                 ; parent term tree per 2026-04-30-ir-formal-grammar.md and
;                 ; the convention used by #1018's missing-template refusal
;                 ; receipt.

operation-kind = "drop" / "free" / "call" / "method" / "ctor" / "dtor" /
                 "send" / "recv" / "yield" / "await" / "throw" / "catch" /
                 tstr

; Locked JCS key order: args_jcs, args_jcs_cid, artifact_kind, callsite_cid,
; concept_cid, concept_name, concept_site_cid, emitted_by, loss_record_cid,
; operation_kind, policy_cid, schema_version, shape_cid, sugar_dict_cid,
; term_position
concept-citation-payload = {
  ? args_jcs:           [* any],
  args_jcs_cid:         cid,
  artifact_kind:        "provekit-concept-citation-comment-sugar",
  ? callsite_cid:       cid,
  concept_cid:          cid,
  ? concept_name:       tstr,
  concept_site_cid:     cid,
  emitted_by:           emitted-by,
  loss_record_cid:      cid,
  operation_kind:       operation-kind,
  ? policy_cid:         cid,
  schema_version:       "1",
  shape_cid:            cid,
  sugar_dict_cid:       cid,
  term_position:        [* uint]
}

; Locked JCS key order: kit_cid, kit_id, kit_kind, target_language,
; target_library_tag
emitted-by = {
  kit_cid:             cid,
  kit_id:              tstr,
  kit_kind:            "realize" / "lift" / "sugar" / tstr,
  target_language:     tstr,
  ? target_library_tag: tstr
}
```

### §1.1 Field semantics

For each field below, the column **CCS parallel** cites the analogous field in
`2026-05-14-contract-comment-sugar.md` §1.1.

| Field | Required | Type | CCS parallel | Meaning and absence rule |
|---|---:|---|---|---|
| `artifact_kind` | yes | const string | `artifact_kind` | MUST be `"provekit-concept-citation-comment-sugar"`. Any other value MUST fail closed per §6. |
| `schema_version` | yes | const string | `schema_version` | MUST be `"1"`. Unknown versions MUST fail closed. |
| `concept_cid` | yes | cid | (no exact parallel; closest is `contract_cid` as identity anchor) | BLAKE3-512 CID of the concept-shape memento this comment cites. Authoritative identity of the transported operation. Missing or malformed: fail closed (§6.3, §6.6). |
| `concept_name` | no | tstr | `fol_text` (diagnostic only) | Optional learned binding (e.g. `"concept:drop"`). Presentation sugar only, never authoritative. Wrong or missing: still relift on `concept_cid` (§5). |
| `operation_kind` | yes | tstr (from `operation-kind`) | `role` | Operation taxonomy entry. The lifter cross-checks with the catalog's recorded `operation_kind` for `concept_cid`. Mismatch: fail closed (§6.8). |
| `shape_cid` | yes | cid | `ir_formula_jcs_cid` | BLAKE3-512 CID of the operation-shape spec resolved from `concept_cid` in the local catalog. The lifter cross-checks with the catalog entry for `concept_cid`. Mismatch: fail closed (§6.7). |
| `concept_site_cid` | yes | cid | `concept_site_cid` | CID of the originating `ConceptSiteMemento` that the operation belongs to. Missing or malformed: fail closed. |
| `callsite_cid` | no | cid | (no exact parallel; concept-site is the static anchor, callsite is the invocation anchor) | CID of the invocation-site memento when the operation is a `call`/`method`/`send`/`recv` whose static concept-site differs from its dynamic invocation. Optional because not every operation has a distinct callsite (e.g. `drop` at scope exit has no separate call shape); when omitted, the relift attaches to `concept_site_cid` alone. |
| `term_position` | yes | [* uint] | (no exact parallel; CCS payload attaches to a syntactic region, this one names a position within the parent term tree) | Path of integer indices into the parent term tree per the convention used by #1018's missing-template refusal receipt and `2026-04-30-ir-formal-grammar.md`. The position the substrate operation would have occupied. Missing or malformed: fail closed. |
| `args_jcs` | no | array | `ir_formula_jcs` | The canonical argument list as structured JSON. When present, the lifter recomputes `args_jcs_cid` over its JCS bytes. A policy MAY omit it and expose only the CID; then relift can recover only if `args_jcs` is resolvable from the local/federated catalog by `args_jcs_cid`. |
| `args_jcs_cid` | yes | cid | `ir_formula_jcs_cid` | BLAKE3-512 CID of the canonical argument-list bytes. Required even when `args_jcs` is omitted. Mismatch with recomputed JCS: fail closed (§6.5). |
| `loss_record_cid` | yes | cid | `loss_record_cid` | CID of the LossRecordMemento attached to this transport site. Empty-loss records still have a CID. Missing: fail closed. |
| `sugar_dict_cid` | yes | cid | `sugar_dict_cid` | CID of the sugar-dict memento that supplied the emission entry. Missing: fail closed. |
| `policy_cid` | no | cid | `policy_cid` | CID of the policy memento that selected comment-transport for this site. Mandatory when the selection was via a policy memento; the policy decision itself is the auditable fact. May be omitted only when the sugar-dict entry is the sole selection authority and the dict's own CID is the policy fact; that absence MUST be encoded by the sugar-dict so that `policy_cid` and `sugar_dict_cid` being equal is forbidden as an obfuscation. When omitted, the lifter MUST verify that the controlling sugar-dict explicitly waives `policy_cid`. |
| `emitted_by` | yes | emitted-by | `emitted_by` | Kit identity and target language/library that emitted the surface. Participates in the payload CID. |

### §1.2 Payload CID

The concept-citation payload's CID is:

```text
payload_cid = blake3-512(JCS(concept-citation-payload))
```

The payload CID MUST be emitted next to the payload as a sibling host-language
comment line carrying the marker `provekit-concept-payload-cid` (§3). The
lifter MUST recompute and compare it. The sibling line MUST appear on the line
immediately following the payload line, with no intervening comment or code.
No whitespace tolerance beyond what `2026-05-14-contract-comment-sugar.md` §3
already permits per host language. An orphan
`provekit-concept-payload-cid:` line with no preceding `provekit-concept:`
payload line MUST be rejected per §6.9.

### §1.3 Exactness condition

A concept-citation relift is exact with respect to operation-identity recovery
only when all of the following hold:

1. `schema_version == "1"`;
2. every required CID field is well formed;
3. `args_jcs` is present or resolvable by `args_jcs_cid`;
4. `blake3-512(JCS(args_jcs)) == args_jcs_cid`;
5. `concept_cid` is present in the local concept-shape catalog;
6. the catalog entry for `concept_cid` records `shape_cid` byte-identical to
   the payload's `shape_cid`;
7. the catalog entry for `concept_cid` records `operation_kind` byte-identical
   to the payload's `operation_kind`;
8. the emitted payload CID recomputes byte-identically; and
9. the lifter places the recovered operation at `term_position` within the
   parent term tree without falling back to a free-form host expression.

If any condition fails, the lifter MUST NOT emit a trusted substrate operation
from the comment. §6 enumerates the refusal modes.

## §2. Payload CID rule

The rule and ordering mirror `2026-05-14-contract-comment-sugar.md` §1.2 and
§3 with a different marker pair:

- `payload_cid = blake3_512(JCS(payload_object))`.
- The lifter recomputes JCS and CID and refuses on mismatch.
- The CID line is **mandatory** (concept-citation is stricter than
  contract-comment, which allows lifter-computed CID recovery): every emitted
  payload MUST be followed by its sibling CID line on the immediately next
  comment line, with no intervening host content.
- No whitespace tolerance beyond what the host language adapter already
  permits per `2026-05-14-contract-comment-sugar.md` §3.
- The two lines MUST share the same host comment family on the same logical
  comment block (a hash-comment payload MUST be followed by a hash-comment
  CID line, not a slash one).

## §3. Source-syntax families

Concept-citation comments live in two host comment families. The marker bytes
are **`provekit-concept:`** and **`provekit-concept-payload-cid:`**. These
markers MUST NOT be conflated with `provekit-contract:` /
`provekit-contract-payload-cid:` (§7). A lifter MUST do a full-marker prefix
match (the full literal string `provekit-concept:` or
`provekit-concept-payload-cid:` followed by exactly one space), not a partial
match on `provekit-c`. This parallels the marker prefix check in
`implementations/python/provekit-lift-python-source/src/provekit_lift_python_source/bind_lifter.py`
around line 386 (`content.startswith("provekit-contract:")`) and MUST be
applied with identical rigor for the new markers.

### §3.1 Hash-comment family (Python, Ruby, shell)

Recommended embedding:

```text
# provekit-concept: {jcs-payload}
# provekit-concept-payload-cid: blake3-512:...
```

Line ordering rule: the CID line immediately follows the payload line.
Whitespace tolerance: exactly one space after the colon for each marker.
Multi-line payload handling: the canonical payload MUST fit on one line; hosts
that cannot carry a long single-line comment SHOULD fall back to the
slash-comment family or refuse emission. Splitting payloads across multiple
adjacent comment lines is NOT permitted under this spec.

### §3.2 Slash-comment family (Rust, C, C++, Java, JS, TypeScript, Go, Swift)

Recommended embedding:

```text
// provekit-concept: {jcs-payload}
// provekit-concept-payload-cid: blake3-512:...
```

Line ordering rule: identical to §3.1. Block comments (`/* ... */`) MAY carry
the payload when the host language has no line-comment surface available; when
block comments are used, the payload MUST be the only machine-readable content
in the block. JSDoc and Javadoc surfaces are NOT defined here; a future
adapter spec MAY extend the lifter's surface set, but the markers above MUST
remain the authoritative carrier in line and block comments.

### §3.3 Marker non-overlap

A lifter MUST NOT consume `provekit-contract:` as concept-citation evidence,
and MUST NOT consume `provekit-concept:` as contract-comment evidence. A line
beginning with `provekit-concept-payload-cid:` MUST NOT be matched by a partial
prefix scan for `provekit-contract:`. Implementations SHOULD reject any line
beginning with `provekit-` that is not one of the four defined markers as
an unrecognized future-extension marker; the rejection MUST emit a diagnostic
but MUST NOT itself fail the surrounding relift.

## §4. Human shorthand from #966: presentation only

#966 defines a human-readable shorthand for concept-name-with-args (for
example `// concept: drop(x)` or `# concept: free(ptr)`). That shorthand MAY
appear in source as a trailing presentation comment adjacent to a
concept-citation block, **but it is presentation only and MUST NOT be
relifted as substrate identity**. Only the JCS payload and its CID line are
authoritative.

Concretely:

1. A lifter MUST NOT emit substrate evidence from a `concept:` shorthand line
   that lacks an accompanying `provekit-concept:` payload + CID block.
2. A lifter MAY use the shorthand line as a low-confidence presentation hint
   when reconstructing diagnostics, the same way `concept_name` is used in
   §0.2.
3. If the shorthand and the payload disagree on the concept name, the payload
   wins and the lifter SHOULD emit a diagnostic noting the disagreement.

This rule parallels the CID-authority rule from #966 / #756: the source-visible
name is editable; the CID is identity.

## §5. CID authority

`concept_name` is a learned binding. The authoritative identity at relift time
is `concept_cid`. Even if `concept_name` is missing or wrong, the comment
relifts to the concept whose CID matches `concept_cid`, provided that concept
is resolvable in the local catalog. Conversely, if `concept_cid` is missing,
malformed, or unresolvable, the relift refuses (§6.3, §6.6) and `concept_name`
alone is insufficient.

The parallel rule lives in #966 / #756: source-visible concept names are an
audit trail and a relift hint, not the identity. Concept-citation comments
preserve that invariant: a renamed concept (catalog binding update) does not
invalidate previously emitted payloads, because the binding from
`concept_name` to substrate identity was never authoritative.

## §6. Fail-closed relift rules

The lifter MUST refuse to mint a trusted substrate operation when any of the
following holds. Each refusal mode names whether the failure drops the single
evidence entry or refuses the surrounding relift, and gives the diagnostic
category that the receipt records. The rigor here matches the existing
contract-comment validation at
`implementations/python/provekit-lift-python-source/src/provekit_lift_python_source/bind_lifter.py`
lines 380 to 420.

| # | Condition | Refusal mode | Receipt category |
|---:|---|---|---|
| 1 | Payload line is not valid UTF-8 JSON. | Drop this evidence entry; surrounding relift continues. | `concept-citation:malformed-json` |
| 2 | `schema_version` is missing or not `"1"`. | Drop this entry. | `concept-citation:unknown-schema-version` |
| 3 | Any CID field is malformed (not `blake3-512:` + 128 hex). | Drop this entry. | `concept-citation:malformed-cid` |
| 4 | Emitted `payload_cid` does not equal `blake3-512(JCS(payload))`. | Drop this entry. | `concept-citation:payload-cid-mismatch` |
| 5 | `args_jcs` present but `blake3-512(JCS(args_jcs))` does not equal `args_jcs_cid`. | Drop this entry. | `concept-citation:args-cid-mismatch` |
| 6 | `concept_cid` is not present in the local concept-shape catalog. | Drop this entry; refuse the surrounding relift only when a policy memento marks `concept_cid` as a required local fact. | `concept-citation:unknown-concept` |
| 7 | Catalog `shape_cid` for `concept_cid` does not equal payload `shape_cid`. | Refuse the surrounding relift; this is a substrate-identity contradiction, not a transport hiccup. | `concept-citation:shape-mismatch` |
| 8 | Catalog `operation_kind` for `concept_cid` does not equal payload `operation_kind`. | Refuse the surrounding relift; same reason as #7. | `concept-citation:operation-kind-mismatch` |
| 9 | Orphan `provekit-concept-payload-cid:` line with no preceding `provekit-concept:` payload line. | Drop the orphan; emit a diagnostic. The surrounding relift continues. | `concept-citation:orphan-cid-line` |

Failing closed means no trusted substrate operation is emitted for that
payload. A kit MAY emit a diagnostic. It MAY also emit low-confidence
presentation evidence through another source-kind when policy allows, but
that evidence MUST NOT reuse the concept-citation payload's exactness claim.

The receipt categories above are the diagnostic shapes the lifter records; the
receipt-memento family that consumes them is owned by the existing receipt
catalog and is not redefined here.

## §7. Distinction from contract-comment sugar and prose comments

Three carriers, three jobs. They MUST NOT be mixed on the same logical
operation.

| Marker | Lifter consumes as | `artifact_kind` | What it identifies |
|---|---|---|---|
| `provekit-concept:` + `provekit-concept-payload-cid:` | concept-citation evidence | `provekit-concept-citation-comment-sugar` | the substrate **operation** at `term_position` |
| `provekit-contract:` + `provekit-contract-payload-cid:` | contract-comment evidence | `provekit-contract-comment-sugar` | a substrate **clause** (pre/post/invariant/throws/observation) attached to a concept site |
| any other comment | prose / programmer trivia (#965) | n/a | not substrate; never authoritative |

Forbidden combinations:

1. Two payload lines of different marker families on the same logical comment
   block. A lifter MUST treat any block containing both
   `provekit-concept:` and `provekit-contract:` markers as ambiguous and
   refuse both entries with the receipt category
   `concept-citation:mixed-carriers`.
2. A concept-citation payload whose `term_position` resolves to a syntactic
   region already carrying contract-comment witnesses for a different concept
   identity. Resolution: the contract-comment witness governs the **clause**;
   the concept-citation witness governs the **operation**; the two coexist
   only when they cite the same `concept_site_cid` and disagree on neither
   `concept_cid` nor `contract_cid`. Disagreement is fail-closed under §6.
3. Use of a `concept_name`-only shorthand (§4) without a paired payload to
   replace a missing concept-citation block. The shorthand MUST NOT be
   promoted to substrate identity under any policy.

Prose comments remain unliftable per #965; this spec does not sanction them.

## §8. Worked examples

### §8.1 Python: a `drop(x)` operation transported into a GC host

Suppose a Rust source has `drop(x)` at the end of a function body. The Rust
realizer's substrate operation has:

```text
concept_cid       = blake3-512:1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a
shape_cid         = blake3-512:2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b
args_jcs_cid      = blake3-512:3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c
concept_site_cid  = blake3-512:4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d
loss_record_cid   = blake3-512:5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e
sugar_dict_cid    = blake3-512:6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f
policy_cid        = blake3-512:7070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070
kit_cid           = blake3-512:8181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181
args              = [{"kind":"var","name":"x"}]
```

The Python realizer cannot express a `drop` natively (Python has no `drop`
surface; the GC discards the binding when scope ends). The realizer emits a
concept-citation block above a Python `pass` statement at the term position
the operation would have occupied:

```python
# provekit-concept: {"args_jcs":[{"kind":"var","name":"x"}],"args_jcs_cid":"blake3-512:3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c","artifact_kind":"provekit-concept-citation-comment-sugar","concept_cid":"blake3-512:1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a","concept_name":"concept:drop","concept_site_cid":"blake3-512:4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d","emitted_by":{"kit_cid":"blake3-512:8181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181818181","kit_id":"provekit-realize-python-core@1.8.0","kit_kind":"realize","target_language":"python"},"loss_record_cid":"blake3-512:5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e","operation_kind":"drop","policy_cid":"blake3-512:7070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070707070","schema_version":"1","shape_cid":"blake3-512:2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b","sugar_dict_cid":"blake3-512:6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f6f","term_position":[3,0]}
# provekit-concept-payload-cid: blake3-512:9292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292929292
pass
```

The `...` content above MUST be treated as full 128-hex `blake3-512:` CIDs;
real emitted payloads MUST contain valid 128-hex CIDs for every CID field.
The JSON is JCS-canonical: alphabetical keys, no whitespace, no comments
inside the payload.

### §8.2 Slash-comment: a `free(ptr)` operation transported into Java

Suppose a C source has `free(ptr)` inside a function body. The substrate
operation has:

```text
concept_cid       = blake3-512:a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9
shape_cid         = blake3-512:b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8
args_jcs_cid      = blake3-512:c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7
concept_site_cid  = blake3-512:d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6
callsite_cid      = blake3-512:e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5
loss_record_cid   = blake3-512:f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4
sugar_dict_cid    = blake3-512:0303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303
policy_cid        = blake3-512:1212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212
kit_cid           = blake3-512:2121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121
args              = [{"kind":"var","name":"ptr"}]
```

The Java realizer emits a slash-comment block above a Java statement that the
JVM discards (Java has no `free`; the GC owns reclamation):

```java
// provekit-concept: {"args_jcs":[{"kind":"var","name":"ptr"}],"args_jcs_cid":"blake3-512:c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7c7","artifact_kind":"provekit-concept-citation-comment-sugar","callsite_cid":"blake3-512:e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5","concept_cid":"blake3-512:a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9a9","concept_name":"concept:free","concept_site_cid":"blake3-512:d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6","emitted_by":{"kit_cid":"blake3-512:2121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121","kit_id":"provekit-realize-java-core@1.8.0","kit_kind":"realize","target_language":"java"},"loss_record_cid":"blake3-512:f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4","operation_kind":"free","policy_cid":"blake3-512:1212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212","schema_version":"1","shape_cid":"blake3-512:b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8","sugar_dict_cid":"blake3-512:0303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303","term_position":[1,2,0]}
// provekit-concept-payload-cid: blake3-512:3434343434343434343434343434343434343434343434343434343434343434343434343434343434343434343434343434343434343434343434343434343434
;
```

Both example payloads are valid JCS (alphabetical keys, no whitespace, no
comments inside JSON, sorted exactly per the §1 locked key order). Real
emitted payloads MUST use full `blake3-512:` CIDs and valid JSON; the
abbreviated hex above is explanatory only and is byte-valid only when
treated as the literal 128-hex strings shown.

### §8.3 Relift

For both examples, the lifter:

1. extracts the JSON payload line and the sibling CID line;
2. recomputes `blake3-512(JCS(payload))` and compares to the emitted CID;
3. recomputes `blake3-512(JCS(args_jcs))` and compares to `args_jcs_cid`;
4. resolves `concept_cid` in the local catalog, then cross-checks
   `shape_cid` and `operation_kind` against the catalog entry;
5. places the recovered operation at `term_position` in the parent term tree;
   and
6. mints `EvidenceMemento` with `source_kind` = `native-surface` and a
   payload-side extension-fields object carrying `concept_cid`,
   `shape_cid`, `args_jcs_cid`, `concept_site_cid`, `callsite_cid` (if
   present), `loss_record_cid`, `sugar_dict_cid`, `policy_cid` (if present),
   and the recomputed `payload_cid`.

The recovered operation cites the same `concept_cid` regardless of which
host language carried the comment. Operation identity has survived the
language hop.

## §9. Conformance for kits

A conforming kit MUST satisfy the following at emit:

1. Select the surface through the normal sugar selection algorithm in
   `2026-05-12-sugar-dict-memento.md` §4, recording `sugar_dict_cid` and
   (when applicable) `policy_cid`; refuse to emit if neither selection
   authority is recordable.
2. Emit the payload line followed immediately by the
   `provekit-concept-payload-cid:` line on the next host comment line, with
   no intervening host content; emit the JCS-canonical payload bytes with
   the §1 locked key order and no whitespace inside the JSON.
3. Compute every CID by the canonical rule
   (`payload_cid = blake3-512(JCS(payload))`,
   `args_jcs_cid = blake3-512(JCS(args_jcs))`) and refuse to emit if any
   value cannot be computed or cited; never emit stale placeholders.

A conforming kit MUST satisfy the following at relift:

1. Apply the full-marker prefix match described in §3.3; never consume
   `provekit-contract:` as concept-citation or vice versa.
2. Apply every fail-closed rule in §6 with the receipt categories listed
   there, and never emit a trusted substrate operation when any condition
   fails.
3. Place the recovered operation at `term_position` within the parent term
   tree and mint `EvidenceMemento` with `source_kind` = `native-surface`,
   preserving every CID listed in §8.3 step 6 in the evidence's extension
   fields so a verifier can reconstruct the audit path
   payload-cid -> concept-cid -> shape-cid -> args-cid -> sugar-dict-cid
   -> policy-cid -> loss-record-cid.

## §10. Non-goals

Per #1021:

- No kit carrier code belongs here.
- Do not change contract-comment sugar.
- Do not trust prose comments as operations.
- Do not add new memento types; this is a sugar surface, not a memento family.
- Do not specify the policy or sugar-selection algorithm; #889 owns that.
