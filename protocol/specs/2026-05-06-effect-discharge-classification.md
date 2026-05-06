# Effect-Discharge Classification — Normative Spec

**Status:** v1.0.1 normative
**Date:** 2026-05-06
**Closes:** #398
**Related:**
- `2026-05-04-linker-daemon-protocol.md` — linker daemon pools discharge mementos
- `2026-05-03-substrate-layers-envelope-header-body.md` — envelope/header/metadata layering
- All per-effect memento specs (aliasing, loop-invariant, try-branch, closure-binding, atomic-ordering, provenance, pin-invariant, drop)
- `2026-04-30-ir-formal-grammar.md` — IrFormula/IrTerm definitions
- `2026-04-30-contract-merge-semantics.md` — compose_function_contracts procedure

---

## §0. Purpose

Define the normative classification of every `Effect` variant in the walk lifter's effect set. The classification determines:

1. Whether the effect **blocks composition** outright.
2. Whether the effect is **dischargeable via memento** — i.e., can be cleared by a signed memento in the verifier pool.
3. The specific **memento type** required for discharge.
4. The **exact match rule** the verifier uses to map an effect to its discharge memento.

The substrate verifier uses this classification to implement `compose_function_contracts_checked` and `check_opacity_effects` deterministically across all kits.

---

## §1. Classification taxonomy

```
Effect classification:
  MementoRequired(memento_kind)  — blocked until a memento of kind K is in the pool
  UnconditionallyBlocked         — refuse composition regardless of pool contents
  Informational                  — recorded for diagnostics, never blocks
```

### §1.1 JCS canonical encoding of the classification table

The classification table (§2) is normative prose, not machine-readable content in this version. However, the **Effect enum itself** participates in the JCS canonical encoding of every `FunctionContractMemento` (via `Effect::to_value()`). Each effect variant's `to_value()` encoding MUST use the `kind` key as the first JCS key in its object, with variant-specific fields following in the declared order in the source enum. The JCS key order for each variant is:

| Effect variant | JCS keys (in order) |
|---|---|
| `Reads` | `kind`, `target` |
| `Writes` | `kind`, `target` |
| `Io` | `kind` |
| `Unsafe` | `kind` |
| `Panics` | `kind` |
| `UnresolvedCall` | `kind`, `name` |
| `OpaqueLoop` | `kind`, `loopCid` |
| `EarlyReturn` | `kind`, `tryCid` |
| `ClosureCapture` | `kind`, `bodyFnCid`, `nCaptures` |
| `PinnedReference` | `kind`, `target` |
| `RawPointerProvenance` | `kind`, `target`, `mutable` |
| `AtomicAccess` | `kind`, `target`, `atomicKind`, `ordering` |
| `PossibleAliasing` | `kind`, `formals` |
| `Drop` | `kind`, `name` |

Adding a new `Effect` variant MUST specify its JCS key order here. Existing variants' key orders MUST NOT change — this is a CID stability guarantee (see §6).

### §1.2 MementoPool::insert() interface

Every `MementoRequired` effect carries a `memento_kind` string. Implementations of `MementoPool::insert()` MUST dispatch on this kind to populate the appropriate discharge index (loop_cid_to_memento, try_cid_to_memento, pin_invariant_to_memento, etc.). The contract is:

```
fn insert(&mut self, memento_cid: String, envelope: Json):
  let kind = memento_kind(envelope)
  match kind:
    "loop-invariant"    => index by header.loopCid → loop_cid_to_memento
    "try-branch"        => index by header.tryCid → try_cid_to_memento
    "closure-binding"   => index by header.bodyFnCid → body_fn_cid_to_memento
    "pin-invariant"     => index by (header.functionCid, header.pinnedTarget) → pin_invariant_to_memento
    "provenance"        => index by header.target → provenance_to_memento
    "atomic-ordering"   => index by (header.target, header.kind, header.ordering) → atomic_ordering_to_memento
    "aliasing"          => index by (header.formalA, header.formalB) → aliasing_to_memento
    "drop"              => index by header.targetType → drop_to_memento
    _                   => no discharge indexing (contract, bridge, etc.)
```

Each memento kind spec defines its own header fields and match keys. The classification table (§2) maps effects to their memento kinds; the insert interface dispatches on those kinds. An effect whose memento kind has no insert arm is effectively undischargeable (same as `UnconditionallyBlocked`).

When `MementoPool::insert()` indexes a Drop classification memento, the index key is the Charon `def_id` of the Drop'd type as recorded in the lifter's IR, NOT the textual name. Verifier lookups under `Effect::Drop { name }` MUST resolve `name` to the same `def_id` form before keying. Cross-crate renames of the underlying type therefore preserve discharge identity; renames at the lifter site that re-resolve to a different `def_id` invalidate prior mementos and require a re-mint.

---

## §2. Per-effect table

| Effect | Classification | Memento Kind | Discharge Match Rule |
|--------|---------------|--------------|----------------------|
| `Reads { target }` | UnconditionallyBlocked | — | — |
| `Writes { target }` | UnconditionallyBlocked | — | — |
| `Io` | UnconditionallyBlocked | — | — |
| `Unsafe` | UnconditionallyBlocked | — | — |
| `Panics` | UnconditionallyBlocked | — | — |
| `UnresolvedCall { name }` | MementoRequired | (none yet) | Always blocked. No discharge path. |
| `OpaqueLoop { loop_cid }` | MementoRequired | `loop-invariant` | `loopCid` ≡ string equality with `header.loopCid` |
| `EarlyReturn { try_cid }` | MementoRequired | `try-branch` | `tryCid` ≡ string equality with `header.tryCid` |
| `ClosureCapture { body_fn_cid, n_captures }` | MementoRequired | `closure-binding` | `bodyFnCid` ≡ string equality with `header.bodyFnCid` |
| `PinnedReference { target }` | MementoRequired | `pin-invariant` | `(function_cid, target)` pair match, `invariant` non-empty |
| `RawPointerProvenance { target, mutable }` | MementoRequired | `provenance` | `target` ≡ string equality with `header.target` |
| `AtomicAccess { target, kind, ordering: None }` | MementoRequired | `atomic-ordering` | `(target, kind, ordering)` triple match |
| `AtomicAccess { target, kind, ordering: Some(_) }` | Informational | — | Ordering statically known. No memento needed. |
| `PossibleAliasing { formals }` | MementoRequired | `aliasing` | Every unordered pair in `formals` has a matching memento |
| `Drop { name }` | MementoRequired | `drop` | `name` ≡ string equality with pool presence check (`has_drop_contract`). The match is anchored by `(function_cid, target_type)` — the `function_cid` prevents false discharge when the same type is dropped across two different function contracts. |
| `Drop { .. }` with trivial/structural drop kind | Informational | — | Trivial/Structural drops do not block composition |

### §2.1 Discharge match rule semantics

All match rules are **exact string equality** unless otherwise noted. Comparisons are case-sensitive. The match rule column describes what field(s) of the effect are compared against what field(s) of the memento header. When multiple fields are listed as a tuple (e.g., `(target, kind, ordering)`), ALL fields must match. A partial match is equivalent to no match — the effect is undischarged.

---

## §3. Category definitions

### §3.1 UnconditionallyBlocked

These effects represent behaviors that the substrate's correctness guarantee cannot model in v1. Any contract carrying one of these effects is treated as ineligible for composition — no memento can discharge them. The function must be proven free of these effects (by construction, or by external verification) before it participates in the federation.

**Why not memento-dischargeable?** Read/Write/IO represent stateful interactions where the order of operations matters. `Unsafe` and `Panics` represent Rust-specific safety invariants that the substrate does not have a formal model for. Discharging them with a signed memento would be an assertion without a mechanical proof.

### §3.2 MementoRequired

These effects represent **honest opacity**: the lifter knows the effect exists but cannot resolve its implications locally. The substrate refuses composition until an external authority provides a discharge memento. This is the primary extensibility mechanism: new memento types can be added without changing the effect system.

### §3.3 Informational

These effects are recorded for diagnostics and audit trails but do not block composition. They either carry fully-resolved information (statically-known atomic ordering, trivial drops) or represent properties that are verified by other means.

---

## §4. Composition refusal order

When the substrate checks a `FunctionContractMemento` for composition eligibility, it applies the classification in this order:

1. **MementoRequired effects** — check each effect against the pool. Any undischarged memento-required effect immediately returns `OpacityError::<kind>NotDischarged`. Return the FIRST undischarged effect (fail-fast). The callers (`compose_function_contracts_checked`) MUST handle this error by returning `Err(...)` without proceeding to subsequent checks.

2. **UnconditionallyBlocked effects** — check if ANY remain after all memento-required effects are discharged. If any unconditionally-blocked effects are present, composition is refused via `Ok(None)` (non-opacity refusal). The caller distinguishes this from opacity failure by examining the `Result` discriminant.

3. **Informational effects** — no check. Always pass.

This order ensures that opacity-discharge mementos are checked BEFORE unconditional refusal, so that a function with an `OpaqueLoop` (memento-required) and `Unsafe` (unconditionally blocked) reports `OpaqueLoop` first — the caller can fix the loop by supplying a loop invariant, then re-check for `Unsafe`.

---

## §5. CID stability and backward compatibility

### §5.1 Adding new Effect variants

The `Effect` enum is content-addressed via JCS canonical encoding. Adding a new variant does NOT change the JCS encoding of existing variants, so existing contract CIDs are stable. The new variant appears only in contracts that the lifter emits for functions that exhibit the new effect.

### §5.2 Adding new discharge memento kinds

A new memento kind (e.g., `"some-future-memento"`) added to the `MementoPool::insert()` dispatch does not break existing pool operations. The new kind is indexed separately; existing kinds' indexes are untouched.

### §5.3 Changing classification of an existing effect

Changing an existing effect's classification (e.g., moving `Unsafe` from `UnconditionallyBlocked` to `MementoRequired("safety")`) is a **breaking change to the composition contract**. Contracts that previously refused composition will now compose (if a safety memento is present). This MUST be gated behind a schema version bump in the `FunctionContractMemento.schemaVersion` field. Composition rules for `schemaVersion: "1"` are locked per this spec; a `schemaVersion: "2"` may reclassify effects under its own rules.

### §5.4 Concurrent addition of effect + memento

When a new effect and its corresponding discharge memento are added concurrently (e.g., `Effect::Drop` + `DropMemento`), both MUST ship in the same PR or in an ordered sequence where the memento spec lands first (so the `MementoPool::insert()` arm exists before any contract carrying the effect reaches the pool). An effect whose memento kind has no `insert()` arm is effectively undischargeable (§1.2).

---

## §6. Cross-references

- The `Effect` enum and `to_value()` implementation live in `implementations/rust/provekit-walk/src/contract.rs`.
- `check_opacity_effects` and `compose_function_contracts_checked` are in the same file.
- `MementoPool::insert()` is in `implementations/rust/provekit-verifier/src/types.rs`.
- Individual memento specs define their own header fields, CID construction rules, and discharge semantics. This classification spec maps effects to memento kinds; the memento specs define what each kind means.
- For effect-specific exceptions (e.g., `PossibleAliasing` requiring ALL pairs discharged, not just one), see the per-effect memento spec.

---

## §7. Out of scope (v1)

- **Composable Read/Write effects.** IO and stateful effects cannot be discharged today.
- **Unsafe-discharge.** The `Unsafe` effect currently blocks unconditionally.
- **Partial discharge.** All memento-required effects for a given contract must be discharged for composition to proceed.
- **Effect ordering semantics.** The order of effects in the JCS array is insertion order from detection. This spec does not define ordering semantics for effects.
