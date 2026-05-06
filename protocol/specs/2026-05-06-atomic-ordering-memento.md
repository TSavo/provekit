# AtomicOrderingMemento — Normative Spec

**Status:** v1.0.0 normative
**Date:** 2026-05-06
**Related:**
- `2026-05-03-substrate-layers-envelope-header-body.md` — envelope/header/metadata layering (v1.4 shape)
- `2026-04-30-memento-envelope-grammar.md` — role taxonomy and CDDL conventions
- `2026-04-30-canonicalization-grammar.md` — JCS canonicalization, normative
- `2026-05-04-linker-daemon-protocol.md` — linker daemon pools discharge mementos

## §0. Purpose

A `FunctionContractMemento` carrying an `Effect::AtomicAccess { target, kind, ordering }` is **opaque with respect to memory ordering** whenever the `ordering` field is `None`: the lifter could not statically determine the ordering (it is passed as a runtime argument to `core::sync::atomic::Ordering` dispatch). The substrate refuses to compose it downstream until the ordering is resolved.

An `AtomicOrderingMemento` is the discharge certificate: it supplies the concrete ordering value (`Relaxed`, `Acquire`, `Release`, `AcqRel`, or `SeqCst`), signed and content-addressed, keyed by the atomic target symbol.

When the lifter CAN determine the ordering statically (the ordering is already `Some(...)` on the effect), no memento is needed — the effect is already fully specified and the substrate composes immediately.

## §1. Wire shape (v1.4 layered)

```cddl
; Imports from the shared type namespace:
;   hash         = tstr .regexp "^[a-z0-9]+-[0-9]+:[0-9a-f]+$"
;   cid          = hash
;   signature    = tstr .regexp "^[a-z0-9]+:[A-Za-z0-9+/]+=*$"
;   pubkey       = tstr .regexp "^[a-z0-9]+:[A-Za-z0-9+/]+=*$"
;   iso8601      = tstr .regexp "^[0-9]{4}-..."

AtomicOrderingMemento = {
  envelope: EnvelopeV14,
  header:   AtomicOrderingHeader,
  metadata: MementoMetadata,
}

EnvelopeV14 = {
  signer:     pubkey,
  declaredAt: iso8601,
  signature:  signature,
}

AtomicOrderingHeader = {
  schemaVersion: "1",
  kind:          "AtomicOrderingMemento",
  target:        tstr,       ; the atomic variable / expression name
  ordering:      Ordering,   ; the concrete ordering
  atomicKind:    AtomicKind, ; Load / Store / Rmw / Cas
  functionCid:   cid,        ; contract CID anchoring the target name
}

MementoMetadata = {
  ? atomicTargetType: tstr,    ; e.g. "AtomicU32", "AtomicBool"
  ? producedBy:       tstr,
  ? producedAt:       iso8601,
}

Ordering = "Relaxed" / "Acquire" / "Release" / "AcqRel" / "SeqCst"
AtomicKind = "Load" / "Store" / "Rmw" / "Cas"
```

## §2. JCS canonical key order (normative)

The canonical JCS object whose BLAKE3-512 hash produces the memento CID uses this normatively declared key order, consistent with the sibling discharge memento family:

```json
{
  "kind": "AtomicOrderingMemento",
  "schemaVersion": "1",
  "target": "counter",
  "ordering": "SeqCst",
  "atomicKind": "Rmw"
}
```

Optional metadata fields (`atomicTargetType`, `producedBy`, `producedAt`) follow in JCS-sorted position after the required fields. When absent, those keys are omitted entirely (not `null`).

### §2.1 Canonical bytes

JCS encodes the memento as a flat object with keys in insertion order. The canonical key order for the memento value is:

```
envelope.header.target       ; concrete target symbol
envelope.header.ordering     ; concrete Ordering
envelope.header.atomicKind   ; Load / Store / Rmw / Cas
envelope.header.schemaVersion  ; "1"
envelope.header.kind         ; "AtomicOrderingMemento"
metadata.atomicTargetType    ; optional
metadata.producedBy          ; optional
metadata.producedAt          ; optional
envelope.metadata            ; (the metadata object)
envelope.header              ; (the header object)
envelope.signer              ; signer pubkey
envelope.declaredAt          ; ISO 8601
envelope.signature           ; Ed25519 sig over JCS({header, metadata})
```

The canonical canonicalization follows the layered assembly rule from `2026-05-03-substrate-layers-envelope-header-body.md` §1:

1. Build `{header, metadata}` as a JCS value.
2. Build `envelope = {signer, declaredAt}`.
3. Sign `JCS({header, metadata})` with Ed25519, producing `signature`.
4. Add `signature` to envelope.
5. Full memento = `{envelope, header, metadata}`.
6. JCS-encode the full memento.
7. CID = `BLAKE3-512(JCS(memento))`.

## §3. Content-addressing (BLAKE3-512)

The memento's CID is:

```
blake3-512:hex(BLAKE3_512(JCS(memento)))
```

## §4. Discharge rule

A function contract carrying `Effect::AtomicAccess { target, kind, ordering: None }` composes if and only if the verifier pool contains an `AtomicOrderingMemento` where:

- `header.target == target` — the affected atomic variable matches.
- `header.atomicKind == kind` — the operation class matches (`Load`, `Store`, `Rmw`, `Cas`).
- `header.ordering` is one of the five valid Ordering values.
- The envelope's signature verifies correctly against the signer's pubkey.

Once discharged, the effect is cleared from the contract's `effects` array and composition proceeds. The ordering value from the memento takes the place of the previously-None `ordering` field on the effect.

A function contract carrying `AtomicAccess { ordering: Some(ord) }` does NOT require a memento — the ordering is already statically known.

## §5. Worked examples

### §5.1 Atomic fetch_add with runtime ordering

```rust
// Source: core::sync::atomic::AtomicU32::fetch_update
// (ordering passed at runtime, statically unknown)
```

**Lifter output:** `Effect::AtomicAccess { target: "counter", kind: Rmw, ordering: None }`.

**Composition:** Refused. The memento pool does not contain an `AtomicOrderingMemento` for target `counter`. The ordering is unresolved.

**Discharge:** Mint an `AtomicOrderingMemento`:
```
header: {
  schemaVersion: "1",
  kind: "AtomicOrderingMemento",
  target: "counter",
  ordering: "SeqCst",
  atomicKind: "Rmw",
}
```

After verification, the effect is discharged and composition proceeds with `ordering: SeqCst`.

### §5.2 Atomic load with known ordering

```rust
// Source: core::sync::atomic::AtomicU32::load(Ordering::Acquire)
// (ordering is a compile-time constant)
```

**Lifter output:** `Effect::AtomicAccess { target: "flag", kind: Load, ordering: Some("Acquire") }`.

**Composition:** Approved immediately. No memento needed — the ordering is statically known.

## §5a. Signing authority and trust model

### §5a.1 Signing ceremony

An `AtomicOrderingMemento` MUST be signed by a curator-level key (`ed25519:<base64>`). The signing key follows the provenance path `secret/provekit/provenance-ed25519`.

1. The author constructs the `header` and `metadata` objects per §1.
2. The author produces JCS-canonical bytes of `{ header, metadata }`.
3. The author signs those bytes with the curator Ed25519 key.
4. The resulting signature is placed in `envelope.signature`.

### §5a.2 Verifier validation

Before an `AtomicOrderingMemento` is admitted into the pool:
1. Extract `envelope.signer`.
2. Recompute JCS of `{ header, metadata }`.
3. Validate Ed25519 `signature` against those bytes and the pubkey.
4. Reject on failure.

## §6. Out of scope (v1)

- **Cross-thread happens-before.** The AtomicOrderingMemento supplies the ordering value per access, but does not model the inter-thread happens-before relation. Two atomics with matching orderings (`SeqCst` + `SeqCst`) have defined inter-thread ordering per the C++20 memory model, but the substrate does not verify this.
- **Fence operations.** `core::sync::atomic::fence(order)` is not yet modeled by the walk lifter.
- **Per-variable ordering.** The memento is keyed on `target`, which is the formal/local name. If a single atomic variable is accessed with different orderings at different sites, each site needs its own memento (or the statically-known path covers it).
- **`ordering: None` for `Load` / `Store` with compile-time constants.** In practice, the Rust atomic API always passes ordering as a `const` argument, so `None` only appears for pointer-indirected or trait-obfuscated call sites. The walk lifter may emit `None` conservatively for those.

## §7. References

- Paper 07 §8 (atomic intrinsics in LLBC)
- `provekit-walk/src/contract.rs` — `Effect::AtomicAccess`, `AtomicKind`
- `provekit-walk/src/llbc_calls.rs` — `detect_atomic_call`, `atomic_kind_for_method`
- `2026-05-04-linker-daemon-protocol.md` — memento pooling
