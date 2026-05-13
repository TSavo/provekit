# TryBranchMemento — Normative Spec

**Status:** v1.4.0 normative
**Date:** 2026-05-05
**Related:**
- `2026-05-03-substrate-layers-envelope-header-body.md` — envelope/header/metadata layering (v1.4 shape)
- `2026-04-30-memento-envelope-grammar.md` — role taxonomy and CDDL conventions (v1.1 flat shape; historical)
- `2026-04-30-canonicalization-grammar.md` — JCS canonicalization, normative
- `2026-04-30-ir-formal-grammar.md` — IrFormula shape; `successPathPred` and `failurePathPred` are IrFormulas
- `2026-05-05-loop-invariant-memento.md` — sibling discharge memento for `Effect::OpaqueLoop`
- `2026-05-05-closure-binding-memento.md` — sibling discharge memento for `Effect::ClosureCapture`

## §0. Purpose

A `FunctionContractMemento` carrying an `Effect::EarlyReturn { try_cid }` is **opaque at that Try-branch site**. The Rust `?` operator desugars to a `match` on `Result`/`Option` (in LLBC: `Switch::Match`) with an early-return on the `Err`/`None` arm. This early return is a genuine control-flow bifurcation: the function's post-condition holds only on the success path; the failure path exits before reaching any caller-visible return site.

The substrate refuses to compose a function carrying this effect downstream until the opacity is discharged. A `TryBranchMemento` is the discharge certificate: it supplies a predicate for the success path and a predicate for the failure path, keyed by the try-site's content CID.

The substrate's composition rule (§5): the contract's `EarlyReturn` effect is cleared if and only if a `TryBranchMemento` whose `tryCid` matches the effect's `try_cid` is present in the pool and passes all validation rules in this spec.

## §1. Wire shape (v1.4 layered)

```cddl
; Shared scalar types from the canonicalization grammar:
;   hash, cid, signature, pubkey, iso8601, ir-formula

try-branch-memento = {
  envelope: {
    signer:     pubkey,
    declaredAt: iso8601,
    signature:  signature    ; over JCS(header ++ metadata)
  },
  header: {
    schemaVersion:         "1",
    kind:                  "try-branch",
    cid:                   cid,         ; DERIVED — see §3
    tryCid:                cid,         ; the blake3-512 CID of the Switch::Match LLBC block
    successPathPredHash:   hash,        ; DERIVED: hash(canonical(metadata.successPathPred))
    failurePathPredHash:   hash         ; DERIVED: hash(canonical(metadata.failurePathPred))
  },
  metadata: {
    successPathPred:   ir-formula,   ; condition that holds when ? propagates Ok/Some
    failurePathPred:   ir-formula,   ; condition that holds when ? early-returns Err/None
    ? resultSort:      tstr,         ; "Result" or "Option" — which wrapper type this site desugars
    ? note:            tstr          ; optional prose annotation
  }
}
```

### §1.1 Field semantics

| Layer    | Field                 | Required | Meaning |
|----------|-----------------------|----------|---------|
| envelope | `signer`              | yes      | `ed25519:<base64>` public key of the minter. |
| envelope | `declaredAt`          | yes      | ISO-8601 UTC minting timestamp. |
| envelope | `signature`           | yes (swarm) | Ed25519 over JCS of `{header, metadata}`. OPTIONAL for local-only use. |
| header   | `schemaVersion`       | yes      | MUST be `"1"`. |
| header   | `kind`                | yes      | MUST be `"try-branch"`. |
| header   | `cid`                 | yes      | Content CID of this memento (DERIVED — §3). |
| header   | `tryCid`              | yes      | The `blake3-512:<hex>` CID emitted into `Effect::EarlyReturn.try_cid` by the LLBC lifter. It is the BLAKE3-512 of the JCS-canonical bytes of the `Switch::Match` block implementing the Try-branch shape. The substrate matches this field against the effect's `try_cid` string exactly. |
| header   | `successPathPredHash` | yes      | DERIVED: `"blake3-512:" ++ hex(BLAKE3-512(JCS(metadata.successPathPred)))`. |
| header   | `failurePathPredHash` | yes      | DERIVED: `"blake3-512:" ++ hex(BLAKE3-512(JCS(metadata.failurePathPred)))`. |
| metadata | `successPathPred`     | yes      | An IrFormula that holds over the function's state when the Try-branch succeeds (the `Ok(v)` or `Some(v)` arm continues). Typically: `exists v. result_inner = v /\ <downstream_condition(v)>`. |
| metadata | `failurePathPred`     | yes      | An IrFormula that holds when the Try-branch fails (early return). Typically: `exists e. early_return = Err(e)` or `early_return = None`. |
| metadata | `resultSort`          | no       | A string hint: `"Result"` or `"Option"`. Informational; the substrate does not validate it. MUST be omitted (not `null`) when absent. |
| metadata | `note`                | no       | Human-readable annotation. MUST be omitted when absent. |

### §1.2 On the two predicates

The `successPathPred` and `failurePathPred` form a partition: together they constrain what the Try-branch site contributes to the function's overall weakest-precondition. The substrate does not mechanically verify that they partition the value space — that is the minter's proof obligation. A minter who supplies an unsound pair produces a certificate that allows composition but does not imply soundness; the soundness argument is delegated to the minter, the same way the ContractMemento's pre/post are the minter's assertion, not the substrate's proof.

## §2. Content-addressing rules

### §2.1 CID construction

The `cid` is the BLAKE3-512 of the JCS-canonical bytes of the `header` object with `cid` elided:

```
cid_input = JCS({
  "failurePathPredHash": <failurePathPredHash>,
  "kind":                "try-branch",
  "schemaVersion":       "1",
  "successPathPredHash": <successPathPredHash>,
  "tryCid":              <tryCid>
})
cid = "blake3-512:" ++ hex(BLAKE3-512(cid_input))
```

(Keys are sorted by JCS rule. `cid` itself is excluded from the input. Optional `metadata` fields are NOT included in the CID input — only the header-tier hashes are.)

**Canonicalization choice (flag for architect review):** Two `TryBranchMemento` mementos for the same `tryCid` and same predicate pair have the same `cid` regardless of `metadata.resultSort` or `metadata.note`. The discharge identity is the pair `(tryCid, successPathPredHash, failurePathPredHash)`; the `resultSort` hint and `note` are non-load-bearing.

### §2.2 Formula hash construction

```
successPathPredHash = "blake3-512:" ++ hex(BLAKE3-512(JCS(metadata.successPathPred)))
failurePathPredHash = "blake3-512:" ++ hex(BLAKE3-512(JCS(metadata.failurePathPred)))
```

The IrFormula MUST be in canonical form per `2026-04-30-ir-formal-grammar.md` before hashing.

## §3. Mint procedure

1. Identify the Try-branch site: obtain the `try_cid` string from `Effect::EarlyReturn` in the target `FunctionContractMemento`.
2. Choose `successPathPred` and `failurePathPred` (IrFormulas).
3. Compute `successPathPredHash` and `failurePathPredHash` per §2.2.
4. Build the `header` object (all fields except `cid`).
5. Compute `cid` per §2.1.
6. Build the `metadata` object.
7. Sign `JCS({header, metadata})` with the minter's Ed25519 key.
8. Emit the full envelope.

**INVARIANT (minting idempotency):** Two mint operations for the same `tryCid` and byte-identical `successPathPred` / `failurePathPred` MUST produce the same `cid`.

## §4. Validation rules

### §4.1 Pass 1: CDDL shape check

Reject if:
- Any required field is missing.
- `kind` is not `"try-branch"` or `schemaVersion` is not `"1"`.
- Any hash field does not match the hash regexp.
- `metadata.successPathPred` or `metadata.failurePathPred` is absent.

### §4.2 Pass 2: DERIVED and REFERENT constraints

**DERIVED (successPathPredHash):** Recompute `hash(JCS(metadata.successPathPred))` and verify it equals `header.successPathPredHash`. Reject on mismatch.

**DERIVED (failurePathPredHash):** Recompute `hash(JCS(metadata.failurePathPred))` and verify it equals `header.failurePathPredHash`. Reject on mismatch.

**DERIVED (cid):** Recompute per §2.1 and verify it equals `header.cid`. Reject on mismatch.

**REFERENT:** The pool MUST contain a `FunctionContractMemento` whose `effects` array contains `{"kind": "early_return", "tryCid": <header.tryCid>}`. A `TryBranchMemento` whose `tryCid` does not match any loaded function contract's opacity effect is a valid envelope but not a valid discharge certificate in context.

**SIGNATURE:** For swarm-distributed mementos, verify `envelope.signature` over `JCS({header, metadata})` against `envelope.signer`. Reject on invalid signature.

## §5. Discharge semantics (substrate behavior)

The substrate's composition guard for `Effect::EarlyReturn`:

```
can_compose_early_return(contract, pool) :=
  for each Effect::EarlyReturn { try_cid } in contract.effects:
    pool contains a valid TryBranchMemento M where M.header.tryCid == try_cid
```

If false, `compose_function_contracts` returns `OpacityError::EarlyReturnNotDischarged { try_cid }`.

A single `TryBranchMemento` discharges exactly one `EarlyReturn` effect. A function with two `?` sites has two `EarlyReturn` effects and requires two distinct `TryBranchMemento` mementos (each keyed by its own `tryCid`).

**Note:** The predicates in the memento are not mechanically verified by the substrate to correctly characterize the `?` site's control flow. The substrate's role is structural: verify the memento exists, the CIDs match, and the DERIVED constraints hold. The minter is responsible for supplying correct predicates; downstream verifiers (SMT solvers, Coq) reason about the predicate's soundness through the normal verification pipeline.

## §6. Worked example

A function `fn parse_and_double(s: &str) -> Result<i64, ParseError>` containing one `?` site where `s.parse::<i64>()` is called:

```json
{
  "envelope": {
    "signer":     "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
    "declaredAt": "2026-05-05T12:00:00Z",
    "signature":  "ed25519:MEUCIQDxxx=="
  },
  "header": {
    "schemaVersion":       "1",
    "kind":                "try-branch",
    "cid":                 "blake3-512:b1c2d3...<128 hex chars>",
    "tryCid":              "blake3-512:ef0102...<128 hex chars>",
    "successPathPredHash": "blake3-512:031415...<128 hex chars>",
    "failurePathPredHash": "blake3-512:162718...<128 hex chars>"
  },
  "metadata": {
    "successPathPred": {
      "kind": "exists",
      "name": "v",
      "sort": { "kind": "primitive", "name": "Int" },
      "body": {
        "kind": "atomic",
        "name": "=",
        "args": [
          { "kind": "var", "name": "parsed_value" },
          { "kind": "var", "name": "v" }
        ]
      }
    },
    "failurePathPred": {
      "kind": "atomic",
      "name": "is-parse-error",
      "args": [
        { "kind": "var", "name": "early_return" }
      ]
    },
    "resultSort": "Result",
    "note": "parse::<i64>() succeeds with parsed integer or fails with ParseError."
  }
}
```

The `cid` is computed as:

```
JCS_bytes = JCS({
  "failurePathPredHash": "blake3-512:162718...",
  "kind":                "try-branch",
  "schemaVersion":       "1",
  "successPathPredHash": "blake3-512:031415...",
  "tryCid":              "blake3-512:ef0102..."
})
cid = "blake3-512:" ++ hex(BLAKE3-512(JCS_bytes))
```

## §7. Cross-references

- The `tryCid` value is produced by `llbc_try::extract_try_branches()` in `implementations/rust/provekit-walk/src/llbc_try.rs`. It is the BLAKE3-512 of the JCS-canonical bytes of the `Switch::Match` block implementing the Try-branch shape.
- The substrate check lives in `EffectSet::check_opacity` and `compose_function_contracts` in `implementations/rust/provekit-walk/src/contract.rs`.
- The pool indexing lives in `MementoPool::insert` in `implementations/rust/provekit-verifier/src/types.rs`, keyed by `header.tryCid`.
- For the `LoopInvariantMemento` (discharges `Effect::OpaqueLoop`), see `2026-05-05-loop-invariant-memento.md`.
- For the `ClosureBindingMemento` (discharges `Effect::ClosureCapture`), see `2026-05-05-closure-binding-memento.md`.
