# ClosureBindingMemento — Normative Spec

**Status:** v1.4.0 normative
**Date:** 2026-05-05
**Related:**
- `2026-05-03-substrate-layers-envelope-header-body.md` — envelope/header/metadata layering (v1.4 shape)
- `2026-05-03-contract-cid-vs-attestation-cid.md` — `bodyFnCid` is a contract CID, not an attestation CID (see §0.1)
- `2026-04-30-memento-envelope-grammar.md` — role taxonomy and CDDL conventions (v1.1 flat shape; historical)
- `2026-04-30-canonicalization-grammar.md` — JCS canonicalization, normative
- `2026-04-30-ir-formal-grammar.md` — IrFormula shape; capture-binding predicates are IrFormulas
- `2026-05-05-loop-invariant-memento.md` — sibling discharge memento for `Effect::OpaqueLoop`
- `2026-05-05-try-branch-memento.md` — sibling discharge memento for `Effect::EarlyReturn`

## §0. Purpose

A `FunctionContractMemento` carrying an `Effect::ClosureCapture { body_fn_cid, n_captures }` is **opaque at that closure-construction site**. The Rust closure body is emitted by Charon as a separate `fun_decl` (a `Fn`/`FnMut`/`FnOnce` trait impl method); its `FunctionContractMemento` is lifted normally. However, the CONTRACT of the CAPTURING function — the one that constructs the closure — cannot be composed downstream because the captures bind the closure body's free variables to the enclosing scope's state, and those bindings are not visible in the body's standalone contract.

A `ClosureBindingMemento` is the discharge certificate: it supplies the per-capture binding predicate that connects the capturing scope's variables to the closure body's captured parameters. Once this memento is in the pool and verified, the substrate can compose contracts through closure boundaries.

### §0.1 `bodyFnCid` is a FunctionContractMemento `cid`, not an attestation CID

The `Effect::ClosureCapture.body_fn_cid` is the `FunctionContractMemento.cid` of the closure body's lifted contract. It is the content CID of the canonical bytes of the memento object itself — what `2026-05-03-contract-cid-vs-attestation-cid.md` calls the "contract CID" — not the attestation CID of any signed envelope wrapping it. This is flagged here as an architect-review item: if future protocol versions introduce a distinction between the substrate's reference to a contract and a signer's attestation of that contract, this field's semantics must be revisited.

## §1. Wire shape (v1.4 layered)

```cddl
; Shared scalar types:
;   hash, cid, signature, pubkey, iso8601, ir-formula

; A single capture binding: one captured variable name bound to a predicate.
capture-binding = {
  captureName:    tstr,          ; the closure body's free variable that is captured
  captureSort:    sort,          ; the sort of the captured value (IrSort per ir-formal-grammar.md)
  bindingPred:    ir-formula,    ; predicate over the capturing scope's state that pins the capture
  bindingPredHash: hash          ; DERIVED: hash(canonical(bindingPred))
}

closure-binding-memento = {
  envelope: {
    signer:     pubkey,
    declaredAt: iso8601,
    signature:  signature        ; over JCS(header ++ metadata)
  },
  header: {
    schemaVersion:     "1",
    kind:              "closure-binding",
    cid:               cid,       ; DERIVED — see §3
    bodyFnCid:         cid,       ; the FunctionContractMemento.cid of the closure body
    nCaptures:         uint,      ; must equal the Effect::ClosureCapture.n_captures value
    captureBindingSetHash: hash   ; DERIVED: hash(canonical(sorted capture-binding array))
  },
  metadata: {
    captureBindings: [+ capture-binding],   ; one entry per captured variable; MUST have n_captures entries
    ? note: tstr
  }
}
```

### §1.1 Field semantics

| Layer    | Field                    | Required | Meaning |
|----------|--------------------------|----------|---------|
| envelope | `signer`                 | yes      | `ed25519:<base64>` public key of the minter. |
| envelope | `declaredAt`             | yes      | ISO-8601 UTC minting timestamp. |
| envelope | `signature`              | yes (swarm) | Ed25519 over JCS of `{header, metadata}`. OPTIONAL for local-only use. |
| header   | `schemaVersion`          | yes      | MUST be `"1"`. |
| header   | `kind`                   | yes      | MUST be `"closure-binding"`. |
| header   | `cid`                    | yes      | Content CID of this memento (DERIVED — §3). |
| header   | `bodyFnCid`              | yes      | The `cid` field of the closure body's `FunctionContractMemento`. This is the contract CID per `2026-05-03-contract-cid-vs-attestation-cid.md`. The substrate matches this against the effect's `body_fn_cid` field exactly. |
| header   | `nCaptures`              | yes      | The count of captured variables. MUST equal `Effect::ClosureCapture.n_captures` for the effect this memento discharges. MUST equal `len(metadata.captureBindings)`. |
| header   | `captureBindingSetHash`  | yes      | DERIVED: `hash(canonical(sorted capture-binding array))`. See §2.2. |
| metadata | `captureBindings`        | yes      | Array of `capture-binding` entries, one per captured variable. MUST be non-empty. Ordering: sorted by `captureName` ascending (lexicographic) for canonical stability. |
| metadata | `note`                   | no       | Human-readable annotation. MUST be omitted when absent. |

### §1.2 `capture-binding` field semantics

| Field            | Required | Meaning |
|------------------|----------|---------|
| `captureName`    | yes      | The name of the captured variable as it appears in the closure body's `FunctionContractMemento` formals. |
| `captureSort`    | yes      | The IrSort of the captured value. |
| `bindingPred`    | yes      | An IrFormula over the CAPTURING function's formals that pins the captured variable's value. Typically `capture = outer_var` or a more complex relational predicate. |
| `bindingPredHash` | yes     | DERIVED: `hash(canonical(bindingPred))`. Substrate-load-bearing; the captureBindingSetHash is computed over these. |

## §2. Content-addressing rules

### §2.1 CID construction

The `cid` is the BLAKE3-512 of the JCS-canonical bytes of the `header` object with `cid` elided:

```
cid_input = JCS({
  "bodyFnCid":              <bodyFnCid>,
  "captureBindingSetHash":  <captureBindingSetHash>,
  "kind":                   "closure-binding",
  "nCaptures":              <nCaptures>,
  "schemaVersion":          "1"
})
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

**Canonicalization choice (flag for architect review):** The `cid` input includes `captureBindingSetHash` (a hash of the full capture-binding array) rather than individual `bindingPredHash` fields. This keeps the header compact regardless of capture count, and means that two mementos with the same `bodyFnCid` but different capture bindings have different `cid`s. The downside: you cannot look up a specific capture by its `bindingPredHash` in the header — only the set-level hash is indexed at the substrate layer.

### §2.2 `captureBindingSetHash` construction

Each `capture-binding` entry's canonical form is:

```
canonical_entry(b) = JCS({
  "bindingPredHash": b.bindingPredHash,
  "captureName":     b.captureName,
  "captureSort":     <sort as canonical IrSort JSON>
})
```

The full set hash covers the sorted array of canonical entries:

```
sorted_entries = sort(canonical_entry(b) for b in captureBindings, by captureName ascending)
captureBindingSetHash = "blake3-512:" ++ hex(BLAKE3-512(JCS(sorted_entries)))
```

**ORDERING constraint:** Entries MUST be sorted by `captureName` ascending in `metadata.captureBindings`. The validator sorts a copy before computing the set hash and rejects if the stored array is not in sorted order.

### §2.3 Per-binding `bindingPredHash` construction

```
bindingPredHash = "blake3-512:" ++ hex(BLAKE3-512(JCS(capture-binding.bindingPred)))
```

## §3. Mint procedure

1. Identify the closure site: obtain `body_fn_cid` and `n_captures` from the `Effect::ClosureCapture` in the target `FunctionContractMemento`.
2. For each captured variable, construct a `capture-binding` entry:
   a. Set `captureName` to the variable name from the closure body's formals.
   b. Set `captureSort` to the variable's IrSort.
   c. Write `bindingPred` as an IrFormula over the capturing scope's state.
   d. Compute `bindingPredHash` per §2.3.
3. Sort the entries by `captureName` ascending.
4. Compute `captureBindingSetHash` per §2.2.
5. Build the `header` object (all fields except `cid`).
6. Compute `cid` per §2.1.
7. Build the `metadata` object.
8. Sign `JCS({header, metadata})` with the minter's Ed25519 key.
9. Emit the full envelope.

**INVARIANT (n_captures check):** The minter MUST verify that `len(captureBindings) == nCaptures == Effect::ClosureCapture.n_captures`. A memento where `nCaptures` disagrees with `len(captureBindings)` is invalid and MUST be rejected by a conforming validator.

**INVARIANT (minting idempotency):** Two mint operations for the same `bodyFnCid` and byte-identical sorted `captureBindings` MUST produce the same `cid`.

## §4. Validation rules

### §4.1 Pass 1: CDDL shape check

Reject if:
- Any required field is missing.
- `kind` is not `"closure-binding"` or `schemaVersion` is not `"1"`.
- Any hash field does not match the hash regexp.
- `metadata.captureBindings` is empty (MUST have at least one entry; zero-capture closures are pure by definition and do not emit the effect).
- `header.nCaptures` does not equal `len(metadata.captureBindings)`.
- Any `capture-binding` entry is missing required fields.

### §4.2 Pass 2: DERIVED and REFERENT constraints

**DERIVED (bindingPredHash per entry):** For each `capture-binding` entry, recompute `hash(JCS(bindingPred))` and verify it equals the entry's `bindingPredHash`. Reject on any mismatch.

**DERIVED (captureBindingSetHash):** Recompute per §2.2 (sort by captureName, build canonical entries, hash the array) and verify it equals `header.captureBindingSetHash`. Reject on mismatch.

**ORDERING constraint:** Verify `metadata.captureBindings` is sorted by `captureName` ascending. Reject if not sorted.

**DERIVED (cid):** Recompute per §2.1 and verify it equals `header.cid`. Reject on mismatch.

**REFERENT (bodyFnCid):** The pool MUST contain a `FunctionContractMemento` (kind `"function-contract"`) whose `cid` equals `header.bodyFnCid`. A `ClosureBindingMemento` referencing a body function not in the pool cannot be validated in context. (This is a pool-level constraint; the memento's own CID and DERIVED constraints are still verifiable in isolation.)

**REFERENT (n_captures match):** The pool MUST contain a `FunctionContractMemento` whose `effects` array contains `{"kind": "closure_capture", "bodyFnCid": <header.bodyFnCid>, "nCaptures": <header.nCaptures>}`. The `nCaptures` in the effect MUST equal `header.nCaptures`.

**SIGNATURE:** For swarm-distributed mementos, verify `envelope.signature` over `JCS({header, metadata})` against `envelope.signer`. Reject on invalid signature.

## §5. Discharge semantics (substrate behavior)

The substrate's composition guard for `Effect::ClosureCapture`:

```
can_compose_closure(contract, pool) :=
  for each Effect::ClosureCapture { body_fn_cid, n_captures } in contract.effects:
    pool contains a valid ClosureBindingMemento M where:
      M.header.bodyFnCid == body_fn_cid
      M.header.nCaptures == n_captures
```

If false, `compose_function_contracts` returns `OpacityError::ClosureCaptureNotDischarged { body_fn_cid }`.

A single `ClosureBindingMemento` discharges exactly one `ClosureCapture` effect keyed by `(bodyFnCid, nCaptures)`. A function that constructs two closures has two `ClosureCapture` effects (with distinct `body_fn_cid` values) and requires two distinct `ClosureBindingMemento` mementos.

**Note on nCaptures ambiguity:** Two closures at the same site that capture different numbers of variables produce effects with the same `body_fn_cid` but different `n_captures`. The pair `(bodyFnCid, nCaptures)` is the discharge key. This pair uniquely identifies the effect in well-formed IR (Charon emits a distinct `fun_decl` per closure body, so `body_fn_cid` values differ across closure sites within the same function). The `nCaptures` field is belt-and-suspenders: the primary key is `bodyFnCid`.

## §6. Worked example

A function `fn apply_offset(offset: i64) -> impl Fn(i64) -> i64` constructs a closure `move |x| x + offset`, capturing `offset`:

```json
{
  "envelope": {
    "signer":     "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
    "declaredAt": "2026-05-05T12:00:00Z",
    "signature":  "ed25519:MEUCIQDxxx=="
  },
  "header": {
    "schemaVersion":          "1",
    "kind":                   "closure-binding",
    "cid":                    "blake3-512:c1d2e3...<128 hex chars>",
    "bodyFnCid":              "blake3-512:f40516...<128 hex chars>",
    "nCaptures":              1,
    "captureBindingSetHash":  "blake3-512:192021...<128 hex chars>"
  },
  "metadata": {
    "captureBindings": [
      {
        "captureName":    "offset",
        "captureSort":    { "kind": "primitive", "name": "Int" },
        "bindingPred": {
          "kind": "atomic",
          "name": "=",
          "args": [
            { "kind": "var", "name": "offset" },
            { "kind": "var", "name": "outer_offset" }
          ]
        },
        "bindingPredHash": "blake3-512:222324...<128 hex chars>"
      }
    ],
    "note": "closure captures outer_offset as offset in the body contract."
  }
}
```

The `captureBindingSetHash` is computed as:

```
canonical_entry = JCS({
  "bindingPredHash": "blake3-512:222324...",
  "captureName":     "offset",
  "captureSort":     {"kind": "primitive", "name": "Int"}
})
sorted_array = [canonical_entry]   ; single element, trivially sorted
captureBindingSetHash = "blake3-512:" ++ hex(BLAKE3-512(JCS(sorted_array)))
```

The `cid` is computed as:

```
JCS_bytes = JCS({
  "bodyFnCid":             "blake3-512:f40516...",
  "captureBindingSetHash": "blake3-512:192021...",
  "kind":                  "closure-binding",
  "nCaptures":             1,
  "schemaVersion":         "1"
})
cid = "blake3-512:" ++ hex(BLAKE3-512(JCS_bytes))
```

## §7. Cross-references

- The `bodyFnCid` and `nCaptures` values are produced by `llbc_closures::extract_closure_captures()` in `implementations/rust/sugar-walk/src/llbc_closures.rs`. The `body_fn_cid` is the `content_cid` of the closure body's `fun_decl` once it is lifted through the normal pipeline.
- The substrate check lives in `EffectSet::check_opacity` and `compose_function_contracts` in `implementations/rust/sugar-walk/src/contract.rs`.
- The pool indexing lives in `MementoPool::insert` in `implementations/rust/sugar-verifier/src/types.rs`, keyed by `header.bodyFnCid`.
- The `bodyFnCid` is the `FunctionContractMemento.cid` of the closure body — the contract CID, not the attestation CID. See `2026-05-03-contract-cid-vs-attestation-cid.md` for the distinction.
- For the `LoopInvariantMemento` (discharges `Effect::OpaqueLoop`), see `2026-05-05-loop-invariant-memento.md`.
- For the `TryBranchMemento` (discharges `Effect::EarlyReturn`), see `2026-05-05-try-branch-memento.md`.
