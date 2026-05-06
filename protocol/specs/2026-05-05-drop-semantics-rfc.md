# Drop Semantics: Design RFC

**Status:** v1.0 RFC (design only, no implementation)
**Date:** 2026-05-05
**Closes:** #417 (#384 C.10)
**Related:**
- `2026-04-30-contract-merge-semantics.md`: `compose_function_contracts` procedure this spec augments
- `2026-05-05-loop-invariant-memento.md`: discharge pattern for opacity effects
- `2026-05-05-try-branch-memento.md`: sibling discharge memento
- `2026-05-05-closure-binding-memento.md`: sibling discharge memento
- #384 Tier C parent: C.10 drop semantics is a gap-closure item
- PR #400: existing `Effect::Drop { name }` as simple opacity marker

---

## §0. Purpose

Rust's `Drop::drop` implementations can panic, allocate, run arbitrary user code, and may have side effects beyond the function's own body. Currently walk treats `drop_in_place` as an ordinary call, composing through it as though the callee's body is the drop code. This is incorrect for any contract that wants to compose through a function with non-trivial Drop behavior on its locals or arguments.

This RFC defines:

1. `DropKind`: a tripartite classification of drop behavior (Trivial, Structural, UserCode).
2. `Effect::Drop`: a new opacity-classified effect carrying the target formal and the DropKind.
3. Lifter behavior: how walk inspects a formal's type to classify the drop and emit the effect.
4. `DropMemento`: a user-authored memento whose presence overrides the pessimistic `Effect::Panics + Effect::Unsafe` chain for a specific Drop impl.
5. Implicit drop-site emission: how the lifter handles scope-exit drops of locals and formals.

The substrate verifier consults this classification when deciding whether drop effects are dischargeable via memento vs. unconditionally blocking.

---

## §1. Effect classification

### §1.1 DropKind enum

```rust
pub enum DropKind {
    /// The type implements Copy or has no destructor. Drop is a no-op.
    Trivial,
    /// The type's drop recursively drops fields (e.g. `String`, `Vec<T>` of
    /// trivial T) but does NOT invoke user-defined `Drop::drop`. The drop
    /// runs, but its behavior is fully determined by structural recursion.
    Structural,
    /// The type has a user-defined `impl Drop`. May panic, allocate, or run
    /// arbitrary user code. The substrate MUST NOT assume any property about
    /// this drop without a DropMemento.
    UserCode,
}
```

### §1.2 Effect::Drop variant

```rust
pub enum Effect {
    // ... existing variants ...
    Drop {
        /// The formal or local name of the value being dropped.
        target: String,
        /// The Charon type path (e.g. "std::fs::File", "Vec<i32>"). Used to
        /// look up the corresponding DropMemento in the verifier pool.
        target_type: String,
        /// Classification of the drop behavior.
        drop_kind: DropKind,
    },
}
```

### §1.3 Opacity classification

`Effect::Drop` is an **opacity-classified** effect. It follows the same discharge discipline as `OpaqueLoop`, `EarlyReturn`, `ClosureCapture`, and `UnresolvedCall`:

- The effect appears in the contract's `effects` set.
- `compose_function_contracts_checked` refuses composition unless the pool contains a matching discharge memento.
- For `UserCode` drops specifically, the discharge memento is a `DropMemento` (see §4) that can additionally override the panic/unsafe chain.

`Trivial` and `Structural` drops do NOT require a `DropMemento` for composition. Their behavior is deterministic and non-observable at the substrate level: Trivial drops are no-ops, and Structural drops decompose into recursive drops of sub-fields which are themselves classified independently.

### §1.4 Classification propagation

For composite types, the lifter computes:

```
classify(ty) =
  if ty has user impl Drop:
    UserCode
  else if ty is Copy or has no Drop and all fields Trivial:
    Trivial
  else:
    max(classify(field_ty) for field_ty in ty.fields())
```

where `max` orders Trivial < Structural < UserCode. The classifier must visit every nested field type via the type-decls table. Cycle detection uses a visited-set keyed on Charon ADT def_id.

This rule guarantees that no UserCode drop can be hidden behind a wrapper struct without a Drop impl. The substrate sees the worst-case classification at the outermost formal, and refuses composition unless a matching DropMemento is in the pool.

---

## §2. Lifter behavior

### §2.1 Explicit `drop_in_place` detection

When walk encounters a Charon-emitted `drop_in_place(x)` call, the lifter must:

1. Inspect `x`'s type via the type-decls table to locate the relevant Adt or primitive type declaration.
2. Walk the type's drop implementation status:
   - If the type is `Copy` or has no `Drop` trait impl AND all its fields are also Trivial (recursive check): emit `Drop { target: "x", drop_kind: Trivial }`.
   - If the type has an `impl Drop`: emit `Drop { target: "x", drop_kind: UserCode }` PLUS `Effect::Panics` and `Effect::Unsafe` (drops can panic; double-panic during unwind is UB).
   - Otherwise (no `Drop` impl, but at least one field has a non-Trivial drop): recursively classify each field's drop. Take the worst-case classification across all fields:
     - All fields Trivial: emit `Drop { target: "x", drop_kind: Trivial }` (already covered above).
     - Worst sub-field is Structural: emit `Drop { target: "x", drop_kind: Structural }`. No `Panics`/`Unsafe` (the recursive Structural drops are themselves classified, and any UserCode would have surfaced).
     - Worst sub-field is UserCode: emit `Drop { target: "x", drop_kind: UserCode }` PLUS `Effect::Panics` and `Effect::Unsafe`. The fact that the OUTER type has no user-defined `Drop` impl is irrelevant: the field's UserCode runs at scope exit.
3. Do NOT attempt `compose_callsite_pre` for a `drop_in_place` call. The pre-contribution of a drop is the drop body's pre, which is opaque to the caller unless a `DropMemento` provides it.

### §2.2 Implicit drop sites

Beyond explicit `drop_in_place`, locals and formals are dropped at scope exit. The lifter must walk every scope-exit point and emit drop effects for any non-trivial typed value.

Two alternatives:

**Alternative A: Per-drop-site emission.** Walk every scope-exit point. For each local with non-trivial drop, emit one `Effect::Drop`. This gives fine-grained error reporting (e.g. "drop of `connection` at line 42 is UserCode and undischarged").

**Alternative B: Aggregated ScopeExitDrops.** Emit ONE effect per function body:

```rust
pub enum Effect {
    // ... existing variants ...
    ScopeExitDrops {
        /// All non-trivial drops that occur at scope exits in this function.
        drops: Vec<ScopedDrop>,
    },
}

pub struct ScopedDrop {
    pub target: String,
    pub drop_kind: DropKind,
    /// Byte-offset or line number for error reporting.
    pub locus: Option<Locus>,
}
```

**RFC recommendation:** Start with Alternative B (aggregated) for v1. Fine-grained per-site emission can be added as a non-breaking enhancement later, since both approaches emit the same classification information; the difference is only in error-reporting granularity. Aggregated emission avoids the combinatorial explosion of walking every scope-exit for every local.

### §2.3 Generic types and monomorphization

For generic types (`Vec<T>`, `Box<T>`, etc.), the drop kind depends on `T`:

```rust
// Vec<T>::drop is Structural regardless of T
// (it drops the heap allocation, then recursively drops each element)
//
// However, the RECURSIVE drop of each element T depends on T's own
// DropKind. The lifter resolves T to its concrete type via Charon's
// monomorphization. After monomorphization, T is known.

fn push_and_drop(v: Vec<String>) { ... }
// After monomorphization: v: Vec<String>
//   Vec<String>::drop → Structural (drops the heap)
//   String::drop → Structural (drops the heap allocation)
//   Each String element's drop → Structural
//   Overall: all Structural, no UserCode
```

**RFC position:** The lifter operates on monomorphized code. By the time Charon emits the IR, all generic parameters are resolved to concrete types. The lifter classifies the concrete type and never sees an unsubstituted generic. No pessimistic fallback is needed.

If Charon encounters a generic function that CANNOT be monomorphized (e.g. a virtual dispatch target), the lifter emits `Effect::Drop { target, drop_kind: UserCode }` as the pessimistic default, plus `Effect::Panics` and `Effect::Unsafe`. This is safe: the substrate refuses composition until a `DropMemento` provides discharge.

---

## §3. DropMemento

A user-authored memento that overrides the pessimistic `Effect::Panics + Effect::Unsafe` chain for a specific Drop impl.

### §3.1 Structure

```rust
pub struct DropMemento {
    /// The fully-qualified type name whose Drop impl this memento covers.
    pub target_type: String,

    /// True if the Drop impl is guaranteed not to panic.
    pub panic_free: bool,

    /// True if the Drop impl does not allocate (heap / arena).
    pub allocation_free: bool,

    /// True if the Drop impl contains no user-defined logic; it only
    /// drops fields recursively. Equivalent to classifying the drop as
    /// Structural rather than UserCode.
    pub user_code_free: bool,
}


```

### §3.2 Discharge rule

When the pool contains a `DropMemento` for `target_type`:

- If `user_code_free`: the `Effect::Drop { drop_kind: UserCode }` is downgraded to Structural. The `Panics` and `Unsafe` effects are discharged.
- If `panic_free` AND `allocation_free` but NOT `user_code_free`: the `Panics` and `Unsafe` effects are discharged. The `Effect::Drop` remains classified as `UserCode` (opacity), and composition requires the drop body's lifted contract to be present in the pool for pre-substitution.
- If neither: the memento provides no discharge. Composition fails with `DropNotDischarged`.

### §3.3 Signing

Like all mementos, `DropMemento` is content-addressed (BLAKE3-512) and signed by the author's signing key. The substrate verifier validates the signature before consulting the memento's claims.

---

## §4. Worked examples

### §4.1 Copy type: no drop effect

```rust
fn f(x: u32) -> u32 { x }
```

**Lifter output:** `x` is `Copy`. No drop at scope exit. No `Effect::Drop` emitted.

### §4.2 Owned String: Structural drop

```rust
fn f(s: String) -> String { s }
```

**Lifter output:**

- `s` is dropped at function exit. `String::drop` frees the heap allocation (no user code).
- Effect: `Drop { target: "s", drop_kind: Structural }`.
- No `Panics` or `Unsafe` effects for this drop. `String::drop` does not panic in normal operation: it deallocates a heap buffer via the global allocator. The classifier verifies that all of `String`'s field drops are themselves Trivial or Structural (the heap pointer is a raw pointer, which is Trivial; the length and capacity are `usize`, also Trivial). Per §1.4, max(Trivial, Trivial, Trivial) = Trivial, but `String`'s explicit (or implicit) `Drop` invocation runs the deallocator. The deallocator is treated as Structural in v1: it does not panic in any specified case under the default `GlobalAlloc` impl, and any allocator that panics would itself need a DropMemento covering its allocator type.

**Composition:** `Structural` drops compose without a `DropMemento`. The substrate recognizes Structural as deterministic.

### §4.3 File handle: UserCode drop

```rust
fn write_and_close(file: File, data: &[u8]) {
    file.write_all(data);
} // File::drop runs here, closes the fd
```

**Lifter output:**

- `file: File`. `File::drop` calls `close(2)` which can fail (returns `io::Error` in Rust, but may panic if the fd is invalid in a debug build).
- Effect: `Drop { target: "file", drop_kind: UserCode }`.
- PLUS: `Effect::Panics`, `Effect::Unsafe` (user-defined Drop may panic; double-panic is UB).

**Composition:** Refused unless the pool contains a `DropMemento` for `std::fs::File` asserting `panic_free: true` OR the pool contains the lifted drop body contract for `File::drop` (so the substrate can compose through the drop's own pre/post).

### §4.4 Vec of Strings: nested Structural

```rust
fn process(v: Vec<String>) { ... }
```

**Lifter output:**

- `v: Vec<String>` is dropped at scope exit.
- `Vec<String>::drop` is Structural (drops the heap buffer, then recursively drops each `String` element).
- Each `String` element's `DropKind` is Structural (drops its own heap buffer).
- Effect: `Drop { target: "v", drop_kind: Structural }`.
- No `Panics` or `Unsafe`. The entire chain is Structural.

**Composition:** `Structural` composes without a `DropMemento`.

### §4.5 Arc<Mutex<T>>: mixed chain

```rust
fn lock_and_use(guard: MutexGuard<'_, u32>) -> u32 {
    *guard
}
```

**Lifter output:**

- `guard: MutexGuard<'_, u32>`. `MutexGuard::drop` calls `Unlock`, which touches the `Mutex`'s internal state.
- `MutexGuard::drop` is UserCode (it is user-defined in `std`).
- Effect: `Drop { target: "guard", drop_kind: UserCode }`.
- PLUS: `Effect::Panics`, `Effect::Unsafe` (drop may panic on poisoned mutex; double-panic is UB).

**Composition:** Refused without a `DropMemento` for `MutexGuard`. The author must provide one asserting the desired properties.

### §4.6 Wrapper without Drop impl, UserCode field

```rust
struct Wrapper { inner: MyPanickyType }
// no impl Drop for Wrapper
// MyPanickyType has impl Drop with panic
```

**Lifter output:**

- `Wrapper` has no user `Drop` impl. The classifier recurses to fields.
- `inner: MyPanickyType` classifies as `UserCode` (user-defined `Drop` impl).
- max(UserCode) = UserCode.
- Effect: `Drop { target: "w", drop_kind: UserCode }`.
- PLUS `Effect::Panics`, `Effect::Unsafe` (the inner field's drop may panic).

**Composition:** Refused without a `DropMemento` for either `Wrapper` or `MyPanickyType`. The wrapper-without-Drop-impl shape does NOT make the drop benign.

---

## §5. Substrate verifier behavior

### §5.1 Pre-cycle check

Before composing any function contract, the verifier checks for `Effect::Drop`:

1. `Trivial`: ignore. No effect on composition.
2. `Structural`: check is pass. No discharge required.
3. `UserCode`: refuse composition UNLESS:
   - The pool contains a `DropMemento` for `target_type` with `user_code_free: true` (downgrades to Structural), OR
   - The pool contains the lifted drop body contract for the type's `Drop::drop` impl, enabling full pre-substitution composition.

### §5.2 Panics/Unsafe override

When the pool contains a `DropMemento` for `target_type`:

- `panic_free: true` → the `Effect::Panics` emitted by the lifter for this drop is discharged.
- `allocation_free: true` → any allocation-related composition blockers are lifted.
- These are additive: a single memento may discharge multiple effects.

### §5.3 Error payload

When composition is refused due to an undischarged drop:

```
DropNotDischarged {
    target: String,
    drop_kind: DropKind,
    reason: DropRefusalReason,
}
```

Where `DropRefusalReason` is:

- `NoMemento`: no DropMemento in pool.
- `PanicsNotDischarged`: memento present but `panic_free: false`.
- `DropBodyNotInPool`: drop body contract needed but not in pool.

---

## §6. Out of scope (v1)

### §6.1 Transitive drop composition

A `Drop::drop` implementation may itself call other functions that have their own contracts. This RFC does NOT define how the substrate composes through a chain of nested drop calls. **RFC position:** drop body composition is deferred to v2. In v1, the substrate refuses composition for any drop with UserCode unless a `DropMemento` asserts `user_code_free: true` (downgrading it to Structural) OR the pool contains a lifted contract for the drop body (the `Drop::drop` implementation has been independently lifted and its contract reviewed). If the drop needs to compose through nested calls, the author must provide pre-lifted contracts for those calls (outside the drop context) and compose them into the caller.

### §6.2 Linear-types "must-drop" enforcement

Rust's drop is MAY-drop: `mem::forget` skips it. This RFC does not enforce that drops MUST run. The substrate does not track liveness of dropped values.

### §6.3 Async drops

Rust's `AsyncDrop` trait is unstable. This RFC does not address async drop futures. When `AsyncDrop` stabilizes, a follow-up RFC must define how async drop bodies compose through the substrate's effect system.

### §6.4 Trait object drops

Drops on `dyn Drop` trait objects are late-bound and not monomorphized. The lifter cannot determine the concrete Drop impl at lift time. **RFC position:** emit `Effect::Drop { target, drop_kind: UserCode }` pessimistically. The substrate refuses composition until a `DropMemento` or the concrete drop body contract is available.

### §6.5 Allocator-panicking drops

Structural drops in v1 assume the global allocator does not panic during deallocation. If a custom allocator's `dealloc` can panic, the substrate currently does not detect this. A future RFC may add an `AllocatorMemento` that carries a panic-free claim about the configured allocator. Until then, code using a panicking allocator must explicitly emit `Effect::Panics` via author-provided memento.

---

## §7. Open questions

### §7.1 Per-drop-site vs. aggregated emission

**Question:** Should the lifter emit one `Effect::Drop` per drop site, or one aggregated `Effect::ScopeExitDrops` per function?

**RFC recommendation:** Aggregated (§2.2, Alternative B) for v1. The classification information is the same in both approaches. Aggregated emission is simpler and avoids combinatorial explosion at scope-exit analysis. Fine-grained per-site emission can be added as a non-breaking enhancement later.

### §7.2 Generic type drop classification before monomorphization

**Question:** When Charon cannot monomorphize a generic type (e.g. trait object, virtual dispatch), should the lifter pessimistically classify as UserCode or fail outright?

**RFC recommendation:** Pessimistic UserCode (§2.3). The substrate refuses composition, which is safe (refuse-unsound). The alternative (fail at lift time) would prevent the function from being lifted at all, which breaks the contract pipeline for other functions in the same crate. Refuse-on-compose is more composable.

### §7.3 `panic_free` as separate effect vs. entangled with Drop

**Question:** Should `Effect::Panics` from drops be a separate effect that can be independently discharged, or should it always be entangled with `Effect::Drop`?

**RFC recommendation:** Entangled initially (§3.2). The `DropMemento` carries `panic_free` as a property. This avoids the need for a separate `PanicFreeMemento` type, which would duplicate the target-type key. If a future version generalizes `panic_free` beyond Drop (e.g. for pure functions), a separate `PanicFreeMemento` can be added without breaking the DropMemento encoding.

### §7.4 Drop body composition tier

**Question:** Should the substrate require the drop body's lifted contract for UserCode drops, or should a DropMemento suffice?

**RFC recommendation:** Tiered (§5.1). A DropMemento with `user_code_free: true` suffices for drops that merely run structural logic. For drops with user logic that interacts with other contracts, the drop body contract is required. This avoids the combinatorial explosion of lifting every `Drop::drop` in the standard library while still supporting the full composition path for types that need it.

### §7.5 Interaction with C.8 aliasing

**Question:** Can Drop implementations access shared mutable state (e.g. `Mutex::drop` unlocks a shared mutex)? Does this interact with the `PossibleAliasing` effect?

**RFC recommendation:** Yes, mutual exclusion primitives are the canonical case. `MutexGuard::drop` calls `Unlock`, which modifies the mutex's internal state (shared with other threads). The lifter should emit `Effect::Drop { target: "guard", drop_kind: UserCode }` BUT NOT `Effect::PossibleAliasing` for the mutex internals, because aliasing is a property of the FORMALS, not of reachable state. Thread-safety of the mutex is a property of the type (the type impls `Sync`) and is not modeled in v1 aliasing.

---

## §8. Cross-references

- The `Effect` enum lives in `implementations/rust/provekit-walk/src/contract.rs`. `Effect::Drop` must be added alongside existing variants.
- The `DropKind` and `DropMemento` types are NEW and should live in `contract.rs` alongside `AliasingMemento`, `AliasingStatus`, etc.
- The `compose_function_contracts_checked` procedure in `contract.rs` must be extended with the drop pre-cycle check from §5.1.
- The `OpacityMementoLookup` trait must be extended with `lookup_drop_memento(target_type: &str) -> Option<&DropMemento>`.
- The `MementoPool` in `provekit-verifier/src/types.rs` must be extended with a drop-memento index.
- For the implementation tracking issues, see #384 C.10 (parent), #417 (this RFC), and the future C.10 lifter implementation issue (to be filed after this RFC lands).
- The existing `Effect::Drop { name }` in PR #400 is a simpler first-pass implementation that does NOT include DropKind classification or DropMemento discharge. This RFC replaces that design. Once this RFC lands and is approved, PR #400 must be updated to match.

---

## §9. RFC disposition

This document is submitted as a **design RFC** under issue #417.

- **Decision body:** Technical governance review.
- **Review period:** Open until 2026-05-12.
- **After acceptance:** A C.10 lifter implementation issue is filed with the exact code shape from this RFC. A C.10 verifier discharge issue is filed with the discharge rules from §5.
- **After rejection:** Sections needing revision are identified; a revised RFC is submitted under the same issue number.
