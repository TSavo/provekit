# ProvenanceMemento — Normative Spec

**Status:** v1.0.0 normative
**Date:** 2026-05-06
**Related:**
- `2026-05-03-substrate-layers-envelope-header-body.md` — envelope/header/metadata layering
- `2026-05-06-atomic-ordering-memento.md` — sibling memento spec, same structural pattern
- `2026-05-06-aliasing-memento.md` — companion memento for raw-pointer aliasing

## §0. Purpose

A `FunctionContractMemento` carrying `Effect::RawPointerProvenance { target, mutable }` signals that the function accepts a raw pointer (`*const T` or `*mut T`) as a formal parameter. The substrate refuses composition for any contract carrying this effect unless a `ProvenanceMemento` is present in the pool.

The memento asserts provenance facts about the pointer at the formal boundary: that it is valid for reads (and writes if `mutable`), non-null, properly aligned, and points to allocated memory. These are properties that rustc's borrow checker verifies for safe references but CANNOT verify for raw pointers — they become deferred proof obligations.

The `ProvenanceMemento` is the discharge certificate: it carries the provenance claims, signed by an authority (the lifter for `unsafe`-block-wrapped dereferences, or an external verifier for general raw-pointer usage).

## §1. Wire shape (v1.4 layered)

```cddl
ProvenanceMemento = {
  envelope: EnvelopeV14,
  header:   ProvenanceHeader,
  metadata: MementoMetadata,
}

EnvelopeV14 = {
  signer:     pubkey,
  declaredAt: iso8601,
  signature:  signature,
}

ProvenanceHeader = {
  schemaVersion: "1",
  kind:          "ProvenanceMemento",
  target:        tstr,       ; the formal parameter name
  functionCid:   cid,        ; contract CID anchoring the target name
  mutable:       bool,       ; true for *mut T, false for *const T
  properties:    [+ ProvenanceProperty],
}

ProvenanceProperty = "NonNull" / "Aligned" / "Allocated" / "Readable" / "Writable"

MementoMetadata = {
  ? allocSite:    tstr,      ; source location of the allocation (e.g. "lib.rs:42")
  ? pointedType:  tstr,      ; the T in *const T / *mut T
  ? producedBy:   tstr,
  ? producedAt:   iso8601,
}
```

## §2. JCS canonical key order (normative)

The canonical JCS object uses this normatively declared key order, consistent with sibling discharge mementos:

```json
{
  "kind": "ProvenanceMemento",
  "schemaVersion": "1",
  "target": "x",
  "functionCid": "blake3-512:...",
  "mutable": false,
  "properties": ["NonNull", "Aligned", "Readable"]
}
```

Optional metadata fields follow in JCS-sorted position after `properties`. When absent, keys are omitted entirely.

### §2.1 Property semantics

| Property    | Meaning |
|-------------|---------|
| `NonNull`   | The pointer is not null. Required for all provenance mementos. |
| `Aligned`   | The pointer is properly aligned for `T` (meets `align_of::<T>()`). |
| `Allocated` | The pointer points to valid allocated memory (heap, stack, or static). |
| `Readable`  | The pointed-to memory is valid for reads (no use-after-free, no uninit). |
| `Writable`  | The pointed-to memory is valid for writes (required when `mutable: true`). |

## §3. Content-addressing

```
CID = blake3-512:hex(BLAKE3_512(JCS(memento)))
```

## §4. Discharge rule

A function contract carrying `Effect::RawPointerProvenance { target, mutable }` composes if and only if the pool contains a `ProvenanceMemento` where:

- `header.target == target` — the affected formal matches.
- `header.mutable == mutable` — the mutability classification matches.
- The envelope signature verifies.
- `"NonNull"` is present in `properties` — raw pointers may be null; the memento must explicitly assert non-null.
- For `mutable: true`, `"Writable"` must be present in `properties`.

Partial discharge is NOT supported in v1. All properties must be asserted.

## §5. Worked examples

### §5.1 Safe `*const T` in unsafe block

```rust
fn read_raw(x: *const u32) -> u32 {
    unsafe { x.read_volatile() }
}
```

**Lifter output:** `Effect::RawPointerProvenance { target: "x", mutable: false }`. Also `Effect::Unsafe` (the unsafe block).

**Composition:** Refused without a `ProvenanceMemento` for target `x`.

**Discharge:** Mint a `ProvenanceMemento`:
```
header: {
  schemaVersion: "1",
  kind: "ProvenanceMemento",
  target: "x",
  mutable: false,
  properties: ["NonNull", "Aligned", "Readable"],
}
```

After verification, the `RawPointerProvenance` effect is cleared. The `Unsafe` effect remains (separate discharge via the unsafe-block is a distinct concern).

### §5.2 Non-null assertion via formal annotation

```rust
/// # Safety
/// `x` must be non-null and point to valid, initialized memory.
unsafe fn write_raw(x: *mut u32, val: u32) {
    x.write(val);
}
```

**Lifter output:** `Effect::RawPointerProvenance { target: "x", mutable: true }`. `Effect::Unsafe`.

**Discharge:** `ProvenanceMemento` with `properties: ["NonNull", "Aligned", "Allocated", "Readable", "Writable"]`.

## §5a. Signing authority and trust model

### §5a.1 Signing ceremony

A `ProvenanceMemento` MUST be signed by a curator-level key. The signing key follows the provenance path `secret/provekit/provenance-ed25519`. Signing proceeds: construct header/metadata → JCS-canonicalize → Ed25519 sign → place in `envelope.signature`.

### §5a.2 Verifier validation

The pool validates the signature before admitting the memento: extract pubkey, recompute JCS, verify Ed25519, reject on failure.

## §6. Out of scope (v1)

- **Per-allocation-site provenance.** The `allocSite` metadata field documents the allocation origin, but the substrate does not yet verify that the pointer's provenance chain resolves to that allocation.
- **Strict-provenance model.** Rust's Strict Provenance experiment (`addr`/`expose_addr`) is not modeled. Pointers carrying exposed-provenance semantics are treated identically to native pointers.
- **Pointer-to-pointer provenance.** Nested raw pointers (`*const *mut T`) emit a single `RawPointerProvenance` for the outermost level. The inner pointer's provenance is not separately modeled in v1.
- **Integer-to-pointer casts.** `ptr::from_exposed_addr` and `as` casts from integers to pointers are not detected by the walk lifter. These produce `Effect::RawPointerProvenance` but the provenance cannot be derived from the type alone.

## §7. Interaction with PossibleAliasing (C.8)

`RawPointerProvenance` and `Effect::PossibleAliasing` are independent. A function can have both: raw pointers create provenance obligations, and the aliasing analysis emits PossibleAliasing for interior-mut-capable shared reference pairs. A single function may require both a `ProvenanceMemento` and an `AliasingMemento` to compose. The substrate checks each effect independently.

## §8. References

- `2026-05-06-atomic-ordering-memento.md` — sibling memento with same layered shape
- `2026-05-06-aliasing-memento.md` — companion memento for reference aliasing
- Paper 07 §8 — raw pointer lifting in LLBC
- `provekit-walk/src/contract.rs` — `Effect::RawPointerProvenance`
