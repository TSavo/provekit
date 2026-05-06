# PinInvariantMemento — Normative Spec

**Status:** v1.0 normative
**Date:** 2026-05-05
**Closes:** #395
**Related:**
- `2026-05-05-loop-invariant-memento.md` — sibling discharge memento for `Effect::OpaqueLoop`
- `2026-05-05-try-branch-memento.md` — sibling discharge memento for `Effect::EarlyReturn`
- `2026-05-05-closure-binding-memento.md` — sibling discharge memento for `Effect::ClosureCapture`
- `2026-04-30-ir-formal-grammar.md` — IrFormula shape; `invariant` is an IrFormula
- `2026-05-04-linker-daemon-protocol.md` — the linker daemon pools discharge mementos for the substrate
- `protocol/provekit-ir.cddl` — CDDL grammar entry locking the canonical wire shape
- #394 — refuse-on-effects discipline this spec hooks into

---

## §0. Purpose

A `FunctionContractMemento` carrying an `Effect::PinnedReference { target }` is opaque at that pinned formal: the substrate cannot reason about the pinned cell's invariance without additional evidence. A `PinInvariantMemento` is the discharge certificate for a single pinned target in a specific function contract: it asserts the invariant the pinned cell satisfies and anchors the memento to the `(function_cid, target)` pair to prevent false discharge across different functions or different pinned types that happen to share the same parameter name.

The substrate's composition rule is: the contract's `PinnedReference` effect is cleared if and only if a `PinInvariantMemento` whose `functionCid` matches the enclosing contract's CID and whose `pinnedTarget` matches the effect's `target` is present in the pool, AND the memento's `invariant` is non-empty. The verifier does NOT validate the `invariant` formula semantically; it only checks non-emptiness and that both match keys (function CID + target name) agree.

---

## §1. Wire shape (v1.4 layered)

The memento follows the substrate-layers spec's `envelope / header / metadata` cut.

```cddl
; Imports from the shared type namespace:
;   cid          = tstr .regexp "^blake3-512:[0-9a-f]+$"
;   signature    = tstr .regexp "^ed25519:[A-Za-z0-9+/]+=*$"
;   pubkey       = tstr .regexp "^ed25519:[A-Za-z0-9+/]+=*$"
;   iso8601      = tstr .regexp "^[0-9]{4}-..."

pin-invariant-memento = {
  envelope: {
    signer:     pubkey,
    declaredAt: iso8601,
    signature:  signature    ; over JCS(header ++ metadata)
  },
  header: {
    schemaVersion: "1",
    kind:          "pin-invariant",
    cid:           cid,           ; DERIVED -- see §3
    functionCid:   cid,           ; the contract CID this memento discharges
    pinnedTarget:  tstr,          ; formal parameter name the effect targets
  },
  metadata: {
    invariant: ir-formula,        ; the pin invariant as an IrFormula
    ? note: tstr                  ; optional prose annotation
  }
}
```

### §1.1 Key order (normative)

The canonical JCS object whose BLAKE3-512 hash produces the memento CID uses this normatively declared key order, consistent with the sibling discharge memento family (`LoopInvariantMemento`, `TryBranchMemento`, `ClosureBindingMemento`):

```json
{
  "kind": "pin-invariant",
  "schemaVersion": "1",
  "functionCid": "blake3-512:...",
  "pinnedTarget": "pin",
  "invariant": "0 <= state"
}
```

The `note` field, when present, is inserted in JCS-sorted position (after `invariant`). When absent, the key is omitted entirely (not `null`).

### §1.2 Field semantics

| Layer    | Field              | Required | Meaning |
|----------|--------------------|----------|---------|
| envelope | `signer`           | yes      | `ed25519:<base64>` public key of the minter. |
| envelope | `declaredAt`       | yes      | ISO-8601 UTC timestamp of minting. |
| envelope | `signature`        | yes      | Ed25519 over JCS of `{header, metadata}`. |
| header   | `schemaVersion`    | yes      | MUST be `"1"`. |
| header   | `kind`             | yes      | MUST be the literal `"pin-invariant"`. |
| header   | `cid`              | yes      | Content CID of this memento (DERIVED -- §3). |
| header   | `functionCid`      | yes      | The CID of the `FunctionContractMemento` this memento is authored for. The substrate matches this field against the enclosing contract's CID exactly. This anchors the memento to the specific function, preventing false discharge when two different functions have pinned parameters with the same name. |
| header   | `pinnedTarget`     | yes      | The formal parameter name that the `Effect::PinnedReference { target }` refers to. Combined with `functionCid`, this forms a `(function_cid, parameter_name)` pair that uniquely identifies the pinned cell. |
| metadata | `invariant`         | yes      | A predicate (IrFormula) that the memento author asserts holds for the pinned cell throughout its lifetime. The verifier validates that the formula string is non-empty before discharging; an empty `invariant` is a malformed memento and does NOT discharge. The formula is opaque to the substrate for semantic purposes. |
| metadata | `note`              | no       | Human-readable annotation. Not substrate-load-bearing; MUST be omitted (not `null`) when absent. |

---

## §2. Content-addressing

### §2.1 CID construction

The memento's CID is the BLAKE3-512 of the JCS-canonical bytes of the header object with `cid` elided and `metadata` merged in:

```
cid_input = JCS({
  "schemaVersion": "1",
  "kind":          "pin-invariant",
  "functionCid":   <functionCid>,
  "pinnedTarget":  <pinnedTarget>,
  "invariant":     <invariant>,
  -- "note": <note>,  // included iff present
})
```

After JCS canonicalization the BLAKE3-512 digest is computed; `cid = "blake3-512:" ++ hex(digest)`.

### §2.2 Canonical key order

The normative key order is declared in §1.1, NOT deferred to JCS lex-sort. This is the same convention used by the three sibling discharge mementos (`loop-invariant-memento`, `try-branch-memento`, `closure-binding-memento`). All four declare key order normatively in their respective §1.1 sections. A cross-spec consistency audit was performed: all four produce byte-identical output under the declared-key-order convention.

---

## §3. Discharge rule

### §3.1 Composition check

When the substrate verifier calls `check_opacity_effects(pool)` on a contract carrying `Effect::PinnedReference { target }`:

1. The pool is queried via `lookup_pin_invariant(function_cid, target)` where `function_cid` is the enclosing contract's CID.
2. If the pool returns `Some(PinInvariantMementoView { .. })` AND `view.invariant` is non-empty: the effect is discharged. Composition proceeds.
3. If the pool returns `None` OR the view's `invariant` is empty: the effect is undischarged. Composition returns `Err(OpacityError::PinInvariantNotDischarged { target })`.

### §3.2 Non-empty invariant requirement

An empty `invariant` string is a malformed memento. The spec requires this check at discharge time (not just at memento validation/load time) to guarantee the invariant field carries semantic content even when pool implementations differ across kits.

### §3.3 Relation to Unpin types

If a formal has type `Pin<P>` where `P: Unpin`, the lifter does NOT emit `Effect::PinnedReference` because `Unpin` means `Pin` is a no-op (the value can be moved out of `Pin` trivially). The `PinnedReference` effect is only emitted for `!Unpin` types, where the pinning has semantic significance.

---

## §4. Auto-minted mementos

The lifter does NOT auto-mint `PinInvariantMemento` entries. Pin invariants are user-authored: the memento author must understand the pinned type's safety contract and assert the invariant appropriate for the composition context. The memento travels in the contract bundle's memento array alongside `LoopInvariantMemento`, `TryBranchMemento`, and `ClosureBindingMemento` entries.

---

## §5. Signing authority and trust model

### §5.1 Signing ceremony

A `PinInvariantMemento` MUST be signed by a curator-level key (`ed25519:<base64>`). The signing key follows the same provenance path established in the substrate's signing pattern: `secret/provekit/provenance-ed25519`.

The signing ceremony is:

1. The author constructs the `header` and `metadata` objects per §1.
2. The author produces the JCS-canonical bytes of the object `{ header (minus cid), metadata }`.
3. The author signs those bytes with the curator Ed25519 key.
4. The resulting signature is placed in `envelope.signature` as `"ed25519:<base64>"`.

### §5.2 Verifier validation

Before a `PinInvariantMemento` is admitted into the pool:

1. The verifier extracts the `envelope.signer` public key.
2. The verifier recomputes the JCS-canonical bytes of `{ header (minus cid), metadata }`.
3. The verifier validates the Ed25519 `signature` against those bytes and the public key.
4. If validation fails, the memento is rejected (not admitted to the pool).

### §5.3 Stdlib rename resilience

If `core::pin::Pin` is renamed or re-exported in a future stdlib version:

- The memento's `functionCid` anchors it to the specific contract, not to the type name.
- If the stdlib change causes Charon to emit a different type identity for the pinned parameter, the lifter will produce a different contract CID, and the existing memento no longer matches.
- The memento author must re-issue the memento against the new contract CID.

---

## §6. Worked examples

### §6.1 Unpin type: no effect emitted

```rust
fn poll_future(pin: Pin<&mut MyFuture>) where MyFuture: Unpin { ... }
```

**Lifter output:** `MyFuture: Unpin`, so `Pin<&mut MyFuture>` is equivalent to `&mut MyFuture`. The lifter does NOT emit `Effect::PinnedReference`. Composition proceeds normally without a memento.

### §6.2 !Unpin type: effect emitted, memento required

```rust
struct MyFuture { state: u32 }
impl !Unpin for MyFuture {}
fn poll(pin: Pin<&mut MyFuture>) -> Poll<()> {
    let this = unsafe { pin.get_unchecked_mut() };
    this.state += 1;
    Poll::Ready(())
}
```

**Lifter output (complete effect set):**

- `Effect::PinnedReference { target: "pin" }` — opacity-dischargeable via PinInvariantMemento
- `Effect::Unsafe` — unconditional block (the `unsafe { ... }` block)
- `Effect::Writes { target: "this.state" }` — unconditional block (the `+=` mutation)

**Discharge outcome:**

After supplying a valid `PinInvariantMemento` for `(function_cid, "pin")`:
- `PinnedReference` is discharged.
- `Unsafe` remains (unconditional block).
- `Writes { target: "this.state" }` remains (unconditional block).

Composition fails at phase 2 because `Effect::Unsafe` and `Effect::Writes` are unconditional blocks. The `PinInvariantMemento` clears the opacity gate on the pinned reference but does NOT remove the other effects. To compose this function, the unsafe block and writes must be factored into separate discharged functions.

### §6.3 Same parameter name, different function: no false discharge

```rust
fn poll_a(pin: Pin<&mut FutureA>) -> Poll<()> { ... }  // contract CID: cid_a
fn poll_b(pin: Pin<&mut FutureB>) -> Poll<()> { ... }  // contract CID: cid_b
```

A `PinInvariantMemento` authored for `(functionCid=cid_a, pinnedTarget="pin")` will NOT discharge `poll_b`'s `PinnedReference` effect because `functionCid` differs. The two functions carry different contract CIDs, so the memento's match fails for `poll_b`. This is the intended behavior: the `(functionCid, pinnedTarget)` pair prevents false discharge across functions that happen to share parameter names.

---

## §7. Out of scope (v1)

### §7.1 Projection-chain validation

Reborrow analysis of `unsafe { pin.get_unchecked_mut() }`, `pin.as_mut().map_unchecked(...)`, and similar projection patterns is NOT validated by the verifier. The `structuralPinning` field present in early drafts was removed because it was CID-load-bearing without composing any composition gate (a decorative hash). It will be re-introduced when the verifier gains Pin-projection reasoning.

### §7.2 Per-kit IR types

This spec defines the Rust-side memento and discharge wiring. Cross-kit propagation of `PinInvariantMemento` to the 11 other language kits is a separate sweep issue. The CDDL entry in `protocol/provekit-ir.cddl` provides the normative wire shape for all kits.

### §7.3 Semantic invariant checking

The `invariant` string is opaque to the substrate. A future verifier revision may add SMT-solving or Pin-projection reasoning that validates this string against the composition context. v1 treats it as an opaque certificate requiring only non-emptiness.

---

## §8. Cross-references

- `PinInvariantMementoView` is the pool's lightweight lookup type in `implementations/rust/provekit-walk/src/contract.rs`. It carries `function_cid`, `pinned_target`, and `invariant`.
- The `OpacityMementoLookup` trait (`contract.rs`) is extended with `fn lookup_pin_invariant(&self, function_cid: &str, target: &str) -> Option<PinInvariantMementoView>`.
- The `Effect::PinnedReference` variant already exists in `contract.rs`. It is moved from "unconditional block" to "opacity dischargeable" in `check_opacity_effects` and added to the phase 2 whitelist in `compose_function_contracts_checked`.
- The `OpacityError::PinInvariantNotDischarged { target }` variant is added.
- `MementoPool` in `implementations/rust/provekit-verifier/src/types.rs` is extended with a `pin_invariant_index` and implements `lookup_pin_invariant`.
- The CDDL grammar entry lives in `protocol/provekit-ir.cddl`.
