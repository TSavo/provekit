# PinInvariantMemento — Normative Spec

**Status:** v1.0.0 normative
**Date:** 2026-05-06
**Related:**
- `2026-05-03-substrate-layers-envelope-header-body.md` — envelope/header/metadata layering
- `2026-05-06-aliasing-memento.md` — companion memento for reference aliasing

## §0. Purpose

A `FunctionContractMemento` carrying `Effect::PinnedReference { target }` signals that the function accepts a `Pin<P>` formal parameter. `Pin` is a structural invariant: the pointee is guaranteed not to move until dropped. The substrate refuses composition for any contract carrying this effect unless a `PinInvariantMemento` is present in the pool.

The memento asserts that the pinning invariant holds: the pointee has been pinned (constructed via `Box::pin`, `pin!`, or `Pin::new_unchecked` with a documented safety justification) and will remain pinned for the duration of the function body. This is a projection-safety proof: without it, `Pin::get_mut` and `Pin::map_unchecked` are unsound.

The `PinInvariantMemento` is the discharge certificate: it carries the pinning guarantee, signed by an authority that has verified the construction site.

## §1. Wire shape (v1.4 layered)

```cddl
PinInvariantMemento = {
  envelope: EnvelopeV14,
  header:   PinInvariantHeader,
  metadata: MementoMetadata,
}

EnvelopeV14 = {
  signer:     pubkey,
  declaredAt: iso8601,
  signature:  signature,
}

PinInvariantHeader = {
  schemaVersion: "1",
  kind:          "PinInvariantMemento",
  target:        tstr,       ; the formal parameter name
  pinSource:     PinSource,  ; how the pin was constructed
  selfReferential: bool,     ; true if Pin<Self> (async fn, Future)
}

PinSource = "BoxPin" / "StackPin" / "UnsafePin" / "HeapPin"

; BoxPin:   constructed via Box::pin(T)
; StackPin: constructed via pin!() macro (stack-pinned)
; UnsafePin: constructed via Pin::new_unchecked
; HeapPin:  constructed via some other heap allocation (Rc::pin, Arc::pin)

MementoMetadata = {
  ? pinSite:      tstr,    ; source location of pin construction
  ? pointeeType:  tstr,    ; the type being pinned
  ? producedBy:   tstr,
  ? producedAt:   iso8601,
}
```

## §2. JCS canonical bytes + key order

1. Build `header = {schemaVersion, kind, target, pinSource, selfReferential}`.
2. Build `metadata = {pinSite?, pointeeType?, producedBy?, producedAt?}`.
3. Build `envelope = {signer, declaredAt}`.
4. Sign `JCS({header, metadata})` with Ed25519.
5. Full memento = `{envelope, header, metadata}`.
6. CID = `BLAKE3-512(JCS(memento))`.

## §3. Content-addressing

```
CID = blake3-512:hex(BLAKE3_512(JCS(memento)))
```

## §4. Discharge rule

A function contract carrying `Effect::PinnedReference { target }` composes if and only if the pool contains a `PinInvariantMemento` where:

- `header.target == target` — the formal parameter name matches.
- The envelope signature verifies.
- `header.selfReferential` must be `true` if the function is an `async fn` or a `Future::poll` implementation (the future's state machine is self-referential through the pinned pointer).

## §5. Worked examples

### §5.1 Async fn with stack-pinned Future

```rust
async fn handle_request(state: State) -> Response { ... }

// Compiler-generated:
// fn handle_request(state: State) -> impl Future<Output = Response> {
//     async move { ... }
// }
```

**Lifter output:** The compiler-generated `Future::poll` function accepts `Pin<&mut Self>`. Effect: `PinnedReference { target: "self" }`.

**Discharge:** A `PinInvariantMemento` for target `"self"` with `pinSource: "StackPin"` and `selfReferential: true`. The memento is auto-minted by the lifter for compiler-generated async state machines (the compiler guarantees the pinning invariant).

### §5.2 Box::pin with manual poll implementation

```rust
let fut = Box::pin(my_async_fn());
// ... later ...
let poll_result = fut.as_mut().poll(cx);
```

**Lifter output:** `Effect::PinnedReference { target: "self" }` on the `poll` function.

**Discharge:** `PinInvariantMemento` with `pinSource: "BoxPin"` and `selfReferential: true`. The memento is minted at the `Box::pin` call site and travels with the boxed future's contract.

### §5.3 Pin::new_unchecked (user-provided safety proof)

```rust
// SAFETY: `x` is heap-allocated and never moved after this point.
let pinned = unsafe { Pin::new_unchecked(&x) };
```

**Lifter output:** `Effect::PinnedReference { target: "pinned" }` PLUS `Effect::Unsafe` (the unsafe block).

**Discharge:** `PinInvariantMemento` with `pinSource: "UnsafePin"` and `selfReferential: false`. The memento must be EXTERNALLY provided (authored by a human or a verification tool), because `Pin::new_unchecked` has no compiler-level safety guarantee.

## §6. Out of scope (v1)

- **Pin projection safety.** `Pin::map_unchecked` and `Pin::get_mut` introduce sub-pins with their own invariants. Each sub-pin projection needs its own `PinInvariantMemento`. The v1 substrate does not automatically propagate pin invariants through projections.
- **Drop guarantee.** Pinned values must be dropped in place. The substrate's drop semantics (#417) do not yet enforce in-place drop for pinned values.
- **Structural pinning.** Types that implement `Unpin` are excluded from pinning effects at the lifter level (the walk lifter checks `is_pin_adt` against type-decls). The memento is only needed for `!Unpin` types.

## §7. References

- `2026-05-06-atomic-ordering-memento.md` — sibling memento, same layered shape
- `2026-05-06-aliasing-memento.md` — companion memento for reference aliasing
- `provekit-walk/src/contract.rs` — `Effect::PinnedReference`
- `provekit-walk/src/llbc_lift.rs` — `is_pin_adt`, formal opacity detection
