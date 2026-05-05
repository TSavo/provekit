# LoopInvariantMemento — Normative Spec

**Status:** v1.4.0 normative
**Date:** 2026-05-05
**Related:**
- `2026-05-03-substrate-layers-envelope-header-body.md` — envelope/header/metadata layering (v1.4 shape this spec conforms to)
- `2026-04-30-memento-envelope-grammar.md` — role taxonomy and CDDL conventions (v1.1 flat shape; historical)
- `2026-04-30-canonicalization-grammar.md` — JCS canonicalization, normative
- `2026-04-30-ir-formal-grammar.md` — IrFormula shape; `invariant` and `decreasingFunction` are IrFormulas
- `2026-05-04-linker-daemon-protocol.md` — the linker daemon pools discharge mementos for the substrate

## §0. Purpose

A `FunctionContractMemento` carrying an `Effect::OpaqueLoop { loop_cid }` is **opaque at that loop**. The substrate refuses to compose it downstream until the opacity is discharged. A `LoopInvariantMemento` is the discharge certificate for a single loop site: it supplies the loop invariant and (optionally) a decreasing function, signed and content-addressed, keyed by the loop's content CID.

The substrate's composition rule (see §5) is: the contract's `OpaqueLoop` effect is cleared if and only if a `LoopInvariantMemento` whose `loopCid` matches the effect's `loop_cid` is present in the pool and passes all validation rules in this spec.

## §1. Wire shape (v1.4 layered)

The memento follows the substrate-layers spec's `envelope / header / metadata` cut.

```cddl
; Imports from the shared type namespace:
;   hash         = tstr .regexp "^[a-z0-9]+-[0-9]+:[0-9a-f]+$"
;   cid          = hash
;   signature    = tstr .regexp "^[a-z0-9]+:[A-Za-z0-9+/]+=*$"
;   pubkey       = tstr .regexp "^[a-z0-9]+:[A-Za-z0-9+/]+=*$"
;   iso8601      = tstr .regexp "^[0-9]{4}-..."
;   ir-formula   ; from 2026-04-30-ir-formal-grammar.md

loop-invariant-memento = {
  envelope: {
    signer:     pubkey,
    declaredAt: iso8601,
    signature:  signature    ; over JCS(header ++ metadata)
  },
  header: {
    schemaVersion: "1",
    kind:          "loop-invariant",
    cid:           cid,           ; DERIVED — see §3
    loopCid:       cid,           ; the blake3-512 CID of the loop's LLBC body block
    invariantHash: hash,          ; DERIVED: hash(canonical(invariant))
    ? decreasingFunctionHash: hash ; DERIVED: hash(canonical(decreasingFunction)), when present
  },
  metadata: {
    invariant:          ir-formula,        ; the loop invariant as an IrFormula
    ? decreasingFunction: ir-formula,      ; the decreasing measure (well-founded ordinal)
    ? note: tstr                           ; optional prose annotation
  }
}
```

### §1.1 Field semantics

| Layer    | Field                    | Required | Meaning |
|----------|--------------------------|----------|---------|
| envelope | `signer`                 | yes      | `ed25519:<base64>` public key of the minter. |
| envelope | `declaredAt`             | yes      | ISO-8601 UTC timestamp of minting. |
| envelope | `signature`              | yes (swarm) | Ed25519 over JCS of `{header, metadata}`. REQUIRED for swarm-distributed mementos; OPTIONAL for local-only mementos consumed in-process. |
| header   | `schemaVersion`          | yes      | MUST be `"1"`. |
| header   | `kind`                   | yes      | MUST be the literal `"loop-invariant"`. |
| header   | `cid`                    | yes      | Content CID of this memento (DERIVED — §3). |
| header   | `loopCid`                | yes      | The `blake3-512:<hex>` CID that was emitted into `Effect::OpaqueLoop.loop_cid` by the lifter. This is the BLAKE3-512 of the JCS-canonical bytes of the loop's LLBC body block. The substrate matches this field against the effect's `loop_cid` string exactly. |
| header   | `invariantHash`          | yes      | DERIVED: `"blake3-512:" ++ hex(BLAKE3-512(JCS(metadata.invariant)))`. Substrate-load-bearing index key. |
| header   | `decreasingFunctionHash` | no       | DERIVED: same construction over `metadata.decreasingFunction`, when that field is present. MUST be absent when `decreasingFunction` is absent. |
| metadata | `invariant`              | yes      | The loop invariant as an IrFormula. MUST be a closed formula (no free variables except those bound by the enclosing `FunctionContractMemento`'s `formals`). |
| metadata | `decreasingFunction`     | no       | A decreasing function from loop state to a well-founded domain (Int or a user-defined ordinal sort). Required for a sound partial-to-total correctness argument; its absence means termination is not certified. |
| metadata | `note`                   | no       | Human-readable annotation. Not substrate-load-bearing; MUST be omitted (not `null`) when absent. |

## §2. Content-addressing rules

### §2.1 CID construction

The memento's `cid` is the BLAKE3-512 of the JCS-canonical bytes of an object that is the `header` with `cid` elided and `metadata` merged in:

```
cid_input = JCS({
  "schemaVersion": "1",
  "kind":          "loop-invariant",
  "loopCid":       <loopCid>,
  "invariantHash": <invariantHash>,
  -- "decreasingFunctionHash": <decreasingFunctionHash>,  // included iff present
})
```

More precisely: the CID input object is the `header` object with the `cid` field removed. Any optional header fields (`decreasingFunctionHash`) appear iff present; they are not force-included as `null`. After JCS canonicalization (sorted keys, no whitespace, UTF-8) the BLAKE3-512 digest is computed; `cid = "blake3-512:" ++ hex(digest)`.

**Canonicalization choice (flag for architect review):** The `cid` input does NOT include `metadata.invariant` or `metadata.decreasingFunction` directly — it includes only their hashes (`invariantHash`, `decreasingFunctionHash`). This mirrors the ContractMemento pattern where `propertyHash` covers `pre/post/inv` hashes, not the raw formulas. Two mementos with the same `loopCid` and same invariant hash will have the same `cid` even if their `metadata.note` fields differ. This is intentional: the note is provenance annotation, not part of the discharge obligation's identity.

### §2.2 `invariantHash` construction

```
invariantHash = "blake3-512:" ++ hex(BLAKE3-512(JCS(metadata.invariant)))
```

The JCS-canonical bytes of the IrFormula object are hashed directly. Implementations MUST produce a canonical IrFormula object per `2026-04-30-ir-formal-grammar.md` before hashing; non-canonical forms produce different hashes and fail the DERIVED constraint check.

### §2.3 `decreasingFunctionHash` construction

Same construction over `metadata.decreasingFunction` when present:

```
decreasingFunctionHash = "blake3-512:" ++ hex(BLAKE3-512(JCS(metadata.decreasingFunction)))
```

## §3. Mint procedure

1. Identify the loop site: obtain the `loop_cid` string from the `Effect::OpaqueLoop` in the target `FunctionContractMemento`.
2. Choose an invariant `I` (an IrFormula over the enclosing function's formals and the loop's variables).
3. Optionally choose a decreasing measure `D` (an IrFormula mapping loop state to an ordinal).
4. Compute `invariantHash = "blake3-512:" ++ hex(BLAKE3-512(JCS(I)))`.
5. If `D` is present, compute `decreasingFunctionHash = "blake3-512:" ++ hex(BLAKE3-512(JCS(D)))`.
6. Build the `header` object (all fields except `cid`).
7. Compute `cid` per §2.1.
8. Build the `metadata` object.
9. Sign `JCS({header, metadata})` with the minter's Ed25519 key.
10. Emit the full envelope.

**INVARIANT (minting idempotency):** Two mint operations for the same `loopCid` and same `invariant` (byte-identical IrFormula) MUST produce the same `cid`. The `signer`, `declaredAt`, and `signature` differ across signers; the `cid` is content-derived and is the same.

## §4. Validation rules

A conforming verifier runs in two passes.

### §4.1 Pass 1: CDDL shape check

Validate the memento against the CDDL in §1. Reject if:
- Any required field is missing.
- `kind` is not the literal `"loop-invariant"`.
- `schemaVersion` is not `"1"`.
- Any hash field does not match the `^[a-z0-9]+-[0-9]+:[0-9a-f]+$` regexp.
- `decreasingFunctionHash` is present but `metadata.decreasingFunction` is absent, or vice versa.

### §4.2 Pass 2: DERIVED and REFERENT constraints

**DERIVED (invariantHash):** Recompute `hash(JCS(metadata.invariant))` and verify it equals `header.invariantHash`. Reject on mismatch.

**DERIVED (decreasingFunctionHash):** When present, recompute `hash(JCS(metadata.decreasingFunction))` and verify it equals `header.decreasingFunctionHash`. Reject on mismatch.

**DERIVED (cid):** Recompute the `cid` per §2.1 and verify it equals `header.cid`. Reject on mismatch.

**REFERENT:** The pool MUST contain a `FunctionContractMemento` (kind `"function-contract"`) whose `effects` array contains an entry `{"kind": "opaque_loop", "loopCid": <header.loopCid>}`. A `LoopInvariantMemento` whose `loopCid` does not match any loaded function contract's opacity effect is malformed in context (not a protocol-level reject, but the substrate refuses to treat it as a valid discharge certificate).

**SIGNATURE:** For swarm-distributed mementos, verify `envelope.signature` over `JCS({header, metadata})` against `envelope.signer`. Reject on invalid signature. Local-only mementos MAY omit the signature; the substrate accepts them for in-process use without swarm verification.

## §5. Discharge semantics (substrate behavior)

The substrate's composition guard for `FunctionContractMemento` is:

```
can_compose(contract, pool) :=
  for each Effect::OpaqueLoop { loop_cid } in contract.effects:
    pool contains a valid LoopInvariantMemento M where M.header.loopCid == loop_cid
```

If `can_compose` is false for either the outer or inner contract in a `compose_function_contracts` call, composition returns an `OpacityError::LoopNotDischarged { loop_cid }`. Composition succeeds only when ALL opacity effects across BOTH contracts are discharged by corresponding mementos in the pool.

A `LoopInvariantMemento` discharges exactly ONE `OpaqueLoop` effect — the one whose `loop_cid` matches `header.loopCid`. A function with two loops has two `OpaqueLoop` effects and requires two distinct `LoopInvariantMemento` mementos.

## §6. Worked example

```json
{
  "envelope": {
    "signer":     "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
    "declaredAt": "2026-05-05T12:00:00Z",
    "signature":  "ed25519:MEUCIQDxxx=="
  },
  "header": {
    "schemaVersion":   "1",
    "kind":            "loop-invariant",
    "cid":             "blake3-512:a1b2c3...<128 hex chars>",
    "loopCid":         "blake3-512:dead01...<128 hex chars>",
    "invariantHash":   "blake3-512:cafe02...<128 hex chars>"
  },
  "metadata": {
    "invariant": {
      "kind": "atomic",
      "name": ">=",
      "args": [
        { "kind": "var", "name": "i" },
        { "kind": "const", "value": 0, "sort": { "kind": "primitive", "name": "Int" } }
      ]
    },
    "decreasingFunction": {
      "kind": "atomic",
      "name": "-",
      "args": [
        { "kind": "var", "name": "n" },
        { "kind": "var", "name": "i" }
      ]
    },
    "note": "Loop index i stays non-negative; n - i decreases toward zero."
  }
}
```

In this example `decreasingFunctionHash` is absent from `header` because... wait, the example has `decreasingFunction` in metadata but no `decreasingFunctionHash` in header. Per §4.1 that is a reject. Corrected:

```json
{
  "envelope": {
    "signer":     "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
    "declaredAt": "2026-05-05T12:00:00Z",
    "signature":  "ed25519:MEUCIQDxxx=="
  },
  "header": {
    "schemaVersion":          "1",
    "kind":                   "loop-invariant",
    "cid":                    "blake3-512:a1b2c3...<128 hex chars>",
    "loopCid":                "blake3-512:dead01...<128 hex chars>",
    "invariantHash":          "blake3-512:cafe02...<128 hex chars>",
    "decreasingFunctionHash": "blake3-512:bead03...<128 hex chars>"
  },
  "metadata": {
    "invariant": {
      "kind": "atomic",
      "name": ">=",
      "args": [
        { "kind": "var", "name": "i" },
        { "kind": "const", "value": 0, "sort": { "kind": "primitive", "name": "Int" } }
      ]
    },
    "decreasingFunction": {
      "kind": "atomic",
      "name": "-",
      "args": [
        { "kind": "var", "name": "n" },
        { "kind": "var", "name": "i" }
      ]
    },
    "note": "Loop index i stays non-negative; n - i decreases toward zero."
  }
}
```

The `cid` is computed as:

```
JCS_bytes = JCS({
  "decreasingFunctionHash": "blake3-512:bead03...",
  "invariantHash":          "blake3-512:cafe02...",
  "kind":                   "loop-invariant",
  "loopCid":                "blake3-512:dead01...",
  "schemaVersion":          "1"
})
cid = "blake3-512:" ++ hex(BLAKE3-512(JCS_bytes))
```

(Keys sorted per JCS rule. `cid` itself is excluded from the input.)

## §7. Cross-references

- The `loopCid` field's value is produced by `llbc_loops::extract_loops()` in `implementations/rust/provekit-walk/src/llbc_loops.rs`. It is the BLAKE3-512 of the JCS-canonical bytes of the loop's LLBC body block.
- The substrate check lives in `EffectSet::check_opacity` and `compose_function_contracts` in `implementations/rust/provekit-walk/src/contract.rs`.
- The pool indexing lives in `MementoPool::insert` in `implementations/rust/provekit-verifier/src/types.rs`, keyed by `header.loopCid`.
- For the `TryBranchMemento` (discharges `Effect::EarlyReturn`), see `2026-05-05-try-branch-memento.md`.
- For the `ClosureBindingMemento` (discharges `Effect::ClosureCapture`), see `2026-05-05-closure-binding-memento.md`.
