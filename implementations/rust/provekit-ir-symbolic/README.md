# provekit-ir-symbolic

The Rust kit's IR-emission surface — runtime-eval lifting of invariants
into ProvekIt's first-order-logic IR.

This crate is the Rust parallel of the TypeScript reference at
`src/ir/symbolic/`. Users authoring `.invariant.rs` files import these
primitives, write invariant code with them, and *running* the code
produces the IR. No syn/quote AST walking. Just function calls and macro
forms.

Cross-language equivalence is the substrate's identity property: the IR
data structures' serde-JSON shape is byte-equivalent to the TS kit's
`IrFormula` JSON for the same logical claim. That equivalence is what
lets `propertyHash` agree across host languages.

## Quickstart

```rust
use provekit_ir_symbolic::property::{begin_collecting, BridgeSpec};
use provekit_ir_symbolic::prelude::*;
use provekit_ir_symbolic::{must, describe, exists, forall};

fn main() {
    let handle = begin_collecting();

    describe!("parseInt", {
        must!("can return zero",
            exists!(s: sorts::string() => eq(parse_int(s), num(0_i64))));

        must!("preserves non-negative ints",
            forall!(n: sorts::int() =>
                implies(gte(n.clone(), num(0_i64)),
                        eq(parse_int(str_("0")), num(0_i64)))));
    });

    let decls = handle.finish();
    println!("{}", serde_json::to_string_pretty(&decls).unwrap());
}
```

## What this crate provides

- `IrFormula`, `IrTerm`, `Sort` — the IR data structures, shaped to
  match the TS kit's JSON byte-for-byte.
- Constants — `num`, `real`, `str_`, `bool_`.
- Built-in primitives — `parse_int`, `parse_float`, `is_nan`, `is_finite`,
  `is_integer`, `abs`, `max`, `min`, `floor`, `ceil`, `sqrt`, `sign`,
  `string_length`, `string_includes`, `array_length`, `array_includes`.
- Term arithmetic — `add`, `sub`, `mul`, `div`, `neg` (lifting numbers
  via `Liftable`).
- Atomic predicates — `eq`, `neq`, `lt`, `lte`, `gt`, `gte`, `is_true`,
  `is_false`.
- Connectives — `and`, `or`, `not`, `implies`, `iff`.
- Quantifiers — `forall_with`, `exists_with`, `for_some` plus `forall!`
  and `exists!` macro forms.
- Collector — `begin_collecting`, `must`, `describe`, `bridge`,
  `property` plus `must!`, `describe!`, `bridge!` macros.

## Binder semantics (load-bearing)

The IR's `varName` for a quantifier is auto-generated as `_x0`, `_x1`,
… via a thread-local counter that mirrors the TS implementation's
`_resetCounter`. The user's identifier in `forall!(x: ... => ...)` is
*only* the binder for the body expression's scope — it is not stored in
the IR. This matches the TS kit's behavior exactly and is required for
cross-language byte-equivalence.

The counter is thread-local, so collection is non-reentrant. Tests and
the lifter must call `_reset_collector` (which also resets the counter)
between runs that need identical IR.

## Test-only canonicalizer

The full byte-deterministic canonicalizer (de Bruijn indexing, AC-norm,
implies-removal, etc.) is downstream. For now this crate uses
`serde_json::to_string_pretty` as the cross-language equivalence stand-
in. A known fixture for `forall(Int, x => x > 0)` is asserted byte-for-
byte against the JSON shape derived from the TS reference; see
`tests/canonical_form_test.rs`.

## Status

- Reference: TypeScript (`src/ir/symbolic/`).
- This crate: 0.1.0 — symbolic primitives only. Producer integrations,
  AST canonicalizer, prompt set, diagnostic translator are downstream
  components of the Rust kit.
- Cargo build: not yet run in CI; verify locally with `cargo test`.

## License

MIT OR Apache-2.0
