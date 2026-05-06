# PinInvariantMemento — Normative Spec

**Status:** v1.0 normative
**Date:** 2026-05-05
**Closes:** #395
**Related:**
- `2026-05-05-loop-invariant-memento.md` — sibling discharge memento for `Effect::OpaqueLoop`
- `2026-05-05-try-branch-memento.md` — sibling discharge memento for `Effect::EarlyReturn`
- `2026-05-05-closure-binding-memento.md` — sibling discharge memento for `Effect::ClosureCapture`
- `2026-04-30-ir-formal-grammar.md` — IrFormula shape; `invariant` is an IrFormula string
- `2026-05-04-linker-daemon-protocol.md` — the linker daemon pools discharge mementos for the substrate
- #394 — refuse-on-effects discipline this spec hooks into

---

## §0. Purpose

A `FunctionContractMemento` carrying an `Effect::PinnedReference { target }` is opaque at that pinned formal: the substrate cannot reason about the pinned cell's invariance without additional evidence. A `PinInvariantMemento` is the discharge certificate for a single pinned target: it asserts the invariant the pinned cell satisfies and whether the pinning is structural (`!Unpin`) or projection-based.

The substrate's composition rule is: the contract's `PinnedReference` effect is cleared if and only if a `PinInvariantMemento` whose `pinnedTarget` matches the effect's `target` is present in the pool. The verifier does NOT validate the `invariant` string semantically; it only checks that the field is non-empty and that the memento target matches.

---

## §1. Memento structure

### §1.1 JCS canonical shape

The memento's canonical JCS object (the object whose BLAKE3-512 hash produces the memento CID) has these keys in JCS-sorted order:

```json
{
  "kind": "pin-invariant",
  "invariant": "<ir-formula predicate string>",
  "pinnedTarget": "<identifier of pinned formal or local>",
  "structuralPinning": true
}
```

### §1.2 Field semantics

| Field               | Required | Meaning |
|---------------------|----------|---------|
| `kind`              | yes      | MUST be the literal `"pin-invariant"`. |
| `pinnedTarget`      | yes      | The formal or local name that the `Effect::PinnedReference { target }` refers to. The substrate matches this field against the effect's `target` string exactly. |
| `invariant`         | yes      | A predicate (IrFormula string) that the memento author asserts holds for the pinned cell throughout its lifetime. The verifier does NOT validate the formula's semantics; it only checks that the string is non-empty. The formula is opaque to the substrate until a future verifier revision adds Pin projection reasoning. |
| `structuralPinning` | yes      | `true` if the pinning follows the `!Unpin` structural discipline (the type is `!Unpin`, so `Pin<P>` cannot be projected without unsafe code). `false` if the pinning is projection-based (the author asserts that a specific field is the pinned projection and that field's invariant is the one relevant to composition). The verifier records this field but does not gate composition on it; it is informational for future Pin-projection verifier extensions. |

---

## §2. Content-addressing

### §2.1 CID construction

The memento's CID is the BLAKE3-512 of the JCS-canonical bytes of the object in §1.1:

```
cid = "blake3-512:" ++ hex(BLAKE3-512(JCS({ "kind": "pin-invariant", "invariant": <s>, "pinnedTarget": <s>, "structuralPinning": <bool> })))
```

The `invariant` string is included as a raw string value in the JCS object, not hashed separately. This is a deliberate design choice: the invariant IS the content of the memento; a property-hash layer is unnecessary for a memento with a single opaque predicate.

### §2.2 JCS key order

JCS sorts keys lexicographically: `invariant`, `kind`, `pinnedTarget`, `structuralPinning`. Implementations MUST emit keys in this order. The `kind` field is NOT first in the JCS sort (unlike the loop-invariant memento where key order is declared normatively); the canonical order is determined by JCS string comparison of key names.

---

## §3. Discharge rule

### §3.1 Composition check

When the substrate verifier calls `check_opacity_effects(pool)` on a contract carrying `Effect::PinnedReference { target }`:

1. The pool is queried via `lookup_pin_invariant(target)`.
2. If the pool returns `Some(PinInvariantMementoView { .. })`: the effect is discharged. Composition proceeds.
3. If the pool returns `None`: the effect is undischarged. Composition returns `Err(OpacityError::PinInvariantNotDischarged { target })`.

### §3.2 No semantic validation of `invariant`

The verifier does NOT attempt to evaluate, prove, or validate the `invariant` formula. It checks only that the `invariant` string is non-empty. Semantic Pin reasoning (projection safety, reborrow chains, drop guarantees) is deferred to a future verifier extension. The memento author is responsible for the correctness of the asserted invariant.

### §3.3 Relation to Unpin types

If a formal has type `Pin<P>` where `P: Unpin`, the lifter does NOT emit `Effect::PinnedReference` because `Unpin` means `Pin` is a no-op (the value can be moved out of `Pin` trivially). The `PinnedReference` effect is only emitted for `!Unpin` types, where the pinning has semantic significance. The memento is therefore only needed for `!Unpin` pinned references.

---

## §4. Auto-minted mementos

The lifter does NOT auto-mint `PinInvariantMemento` entries. Pin invariants are user-authored: the memento author must understand the pinned type's safety contract and assert the invariant appropriate for the composition context.

The memento travels in the contract bundle's memento array alongside `LoopInvariantMemento`, `TryBranchMemento`, and `ClosureBindingMemento` entries. The linker daemon loads all mementos from the bundle into the pool before composition.

---

## §5. Worked examples

### §5.1 Unpin type: no effect emitted

```rust
fn poll_future(pin: Pin<&mut MyFuture>) where MyFuture: Unpin { ... }
```

**Lifter output:** `MyFuture: Unpin`, so `Pin<&mut MyFuture>` is equivalent to `&mut MyFuture`. The lifter does NOT emit `Effect::PinnedReference`. Composition proceeds normally without a memento.

### §5.2 !Unpin type: effect emitted, memento required

```rust
struct MyFuture { state: u32 }
impl !Unpin for MyFuture {}
fn poll(pin: Pin<&mut MyFuture>) -> Poll<()> {
    let this = unsafe { pin.get_unchecked_mut() };
    this.state += 1;
    Poll::Ready(())
}
```

**Lifter output:**

- `pin: Pin<&mut MyFuture>` where `MyFuture: !Unpin`.
- Effect: `PinnedReference { target: "pin" }`.

**Composition:** Requires a `PinInvariantMemento` with `pinnedTarget = "pin"` and a non-empty `invariant` string (e.g. `"0 <= state"`). Without the memento, composition returns `PinInvariantNotDischarged { target: "pin" }`.

### §5.3 Projection-pinned case

```rust
fn poll_field(pin: Pin<&mut MyStruct>) where MyStruct: !Unpin {
    let field = unsafe { pin.map_unchecked_mut(|s| &mut s.field) };
    // field is Pin<&mut FieldType>
}
```

**Lifter output:** `PinnedReference { target: "pin" }` from the original pinned formal. The lifter does NOT emit a separate effect for the projected field; projection reasoning is deferred to a future verifier extension. The memento author asserts the invariant on the ORIGINAL pinned formal, not the projection.

**RFC position for v1:** Projection-pinned sub-fields are not independently tracked. The memento covers the root pinned formal. A future spec revision may add per-field Pin projection mementos.

---

## §6. Out of scope (v1)

### §6.1 Projection-chain validation

Reborrow analysis of `unsafe { pin.get_unchecked_mut() }`, `pin.as_mut().map_unchecked(...)`, and similar projection patterns is NOT validated by the verifier. The `structuralPinning` field records the memento author's intent but the verifier does not enforce it.

### §6.2 Per-kit IR types

This spec defines the Rust-side memento and discharge wiring. Cross-kit propagation of `PinInvariantMemento` to the 11 other language kits is a separate sweep issue (mirroring #418 for `Sort::Region`).

### §6.3 Semantic invariant checking

The `invariant` string is opaque to the substrate. A future verifier revision may add SMT-solving or Pin-projection reasoning that validates this string against the composition context. v1 treats it as an opaque certificate.

---

## §7. Cross-references

- `PinInvariantMemento` struct lives in `implementations/rust/provekit-walk/src/contract.rs` alongside other memento types.
- `PinInvariantMementoView` is the pool's lightweight lookup type. It carries `pinned_target`, `invariant`, `structural_pinning`.
- The `OpacityMementoLookup` trait (`contract.rs`) is extended with `fn lookup_pin_invariant(&self, target: &str) -> Option<PinInvariantMementoView>`.
- The `Effect::PinnedReference` variant already exists in `contract.rs` (line 115). It is moved from "unconditional block" to "opacity dischargeable" in `check_opacity_effects` and added to the phase 2 whitelist in `compose_function_contracts_checked`.
- The `OpacityError::PinInvariantNotDischarged { target }` variant is added.
