# Effect-Discharge Classification — Normative Spec

**Status:** v1.0.0 normative
**Date:** 2026-05-06
**Related:**
- `2026-05-04-linker-daemon-protocol.md` — linker daemon pools discharge mementos
- `2026-05-03-substrate-layers-envelope-header-body.md` — envelope/header/metadata layering
- All per-effect memento specs (aliasing, loop-invariant, try-branch, closure-binding, atomic-ordering, provenance, pin-invariant, drop)

## §0. Purpose

Define the normative classification of every `Effect` variant in the walk lifter's effect set. The classification determines:

1. Whether the effect **blocks composition** outright.
2. Whether the effect is **dischargeable via memento** — i.e., can be cleared by a signed memento in the verifier pool.
3. The specific **memento type** required for discharge.

The substrate verifier uses this classification to implement `compose_function_contracts_checked` and `check_opacity_effects` deterministically across all kits.

## §1. Classification taxonomy

```
Effect classification:
  MementoRequired(memento_kind)  — blocked until a memento of kind K is in the pool
  UnconditionallyBlocked         — refuse composition regardless of pool contents
  Informational                  — recorded for diagnostics, never blocks
```

## §2. Per-effect table

| Effect | Classification | Memento Kind | Discharge Rule |
|--------|---------------|--------------|----------------|
| `Reads { target }` | UnconditionallyBlocked | — | Composition refused. Function has observable state reads. |
| `Writes { target }` | UnconditionallyBlocked | — | Composition refused. Function has observable state writes. |
| `Io` | UnconditionallyBlocked | — | Composition refused. Function performs I/O. |
| `Unsafe` | UnconditionallyBlocked | — | Composition refused. Function contains unsafe code. |
| `Panics` | UnconditionallyBlocked | — | Composition refused. Function may panic. |
| `UnresolvedCall { name }` | MementoRequired | (none yet) | Always blocked in v1. No discharge path exists. |
| `OpaqueLoop { loop_cid }` | MementoRequired | `LoopInvariantMemento` | `loopCid` matches memento header. |
| `EarlyReturn { try_cid }` | MementoRequired | `TryBranchMemento` | `tryCid` matches memento header. |
| `ClosureCapture { body_fn_cid, n_captures }` | MementoRequired | `ClosureBindingMemento` | `bodyFnCid` matches memento header. |
| `PinnedReference { target }` | MementoRequired | `PinInvariantMemento` | `target` matches memento header. |
| `RawPointerProvenance { target, mutable }` | MementoRequired | `ProvenanceMemento` | `target` + `mutable` match memento header. |
| `AtomicAccess { target, kind, ordering: None }` | MementoRequired | `AtomicOrderingMemento` | `target` + `atomicKind` match memento header. |
| `AtomicAccess { target, kind, ordering: Some(_) }` | Informational | — | Ordering statically known. No memento needed. |
| `PossibleAliasing { formals }` | MementoRequired | `AliasingMemento` | Every pair in `formals` has a matching memento. |
| `Drop { target, drop_kind, target_type }` | MementoRequired | `DropMemento` (or lifted drop contract) | `target_type` matches, and `user_code_free: true` or pool contains drop body contract. |
| `Drop { drop_kind: Trivial or Structural }` | Informational | — | No memento needed. Structural drops are deterministically safe. |

## §3. Category definitions

### §3.1 UnconditionallyBlocked

These effects represent behaviors that the substrate's correctness guarantee cannot model in v1. Any contract carrying one of these effects is treated as ineligible for composition — no memento can discharge them. The function must be proven free of these effects (by construction, or by external verification) before it participates in the federation.

**Why not memento-dischargeable?** Read/Write/IO represent stateful interactions where the order of operations matters. `Unsafe` and `Panics` represent Rust-specific safety invariants that the substrate does not have a formal model for. Discharging them with a signed memento would be an assertion without a mechanical proof.

### §3.2 MementoRequired

These effects represent **honest opacity**: the lifter knows the effect exists but cannot resolve its implications locally. The substrate refuses composition until an external authority provides a discharge memento. This is the primary extensibility mechanism: new memento types can be added without changing the effect system.

### §3.3 Informational

These effects are recorded for diagnostics and audit trails but do not block composition. They either carry fully-resolved information (statically-known atomic ordering, trivial drops) or represent properties that are verified by other means (the `SelfReferential` flag on a PinInvariant memento).

## §4. Composition refusal order

When the substrate checks a `FunctionContractMemento` for composition eligibility, it applies the classification in this order:

1. **MementoRequired effects** — check each effect against the pool. Any undischarged memento-required effect immediately returns `OpacityError::<kind>NotDischarged`. Return the FIRST undischarged effect.
2. **UnconditionallyBlocked effects** — check if ANY remain after memento discharge. If any are present, composition is refused. Return `CompositionError::ImpureEffect { effect_kind }`.
3. **Informational effects** — no check. Always pass.

This order ensures that opacity-discharge mementos are checked BEFORE unconditional refusal, so that a function with an `OpaqueLoop` (which is memento-required) and `Unsafe` (unconditionally blocked) reports `OpaqueLoop` first — the caller can fix the loop by supplying a loop invariant, then re-check for `Unsafe`.

## §5. Out of scope (v1)

- **Composable Read/Write effects.** IO and stateful effects cannot be discharged today. A future `EffectMemento` could encode a linear-resource discipline where reads/writes compose in a specified order.
- **Unsafe-discharge.** The `Unsafe` effect currently blocks unconditionally. A future `SafetyMemento` could carry a formal proof that the unsafe block upholds Rust's safety invariants.
- **Partial discharge.** All memento-required effects for a given contract must be discharged for composition to proceed. Partial discharge (discharge some effects, leave others) is not supported in v1.
