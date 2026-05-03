# ref-int32-arithmetic-v1

The canonical reference contract for 32-bit signed integer arithmetic, with explicit overflow semantics.

> **Status:** stub.

## CID

`blake3-512:bafy...ref-int32-arithmetic-v1` *(placeholder)*

## What it claims

> An integer `n` is in the int32 range if and only if `INT32_MIN ≤ n ≤ INT32_MAX`, where `INT32_MIN = -2147483648` and `INT32_MAX = 2147483647`.
>
> Operations on int32 values that produce results outside this range are *defined to wrap modulo 2³²* (two's complement semantics).

This reference exists primarily as a substrate for other references that depend on int32 semantics: `ref-parseInt-v1`, parsing-style references, arithmetic-style references.

## Canonical IR (sketch)

```
{
  "kind": "contract",
  "name": "ref-int32-arithmetic-v1",
  "version": 1,
  "params": [
    {"name": "n", "sort": {"kind": "primitive", "name": "Int"}}
  ],
  "post": {
    "kind": "and",
    "left": {
      "kind": "atomic", "predicate": "ge",
      "args": [{"kind": "var", "name": "n"}, {"kind": "const", "type": "Int", "value": -2147483648}]
    },
    "right": {
      "kind": "atomic", "predicate": "le",
      "args": [{"kind": "var", "name": "n"}, {"kind": "const", "type": "Int", "value": 2147483647}]
    }
  }
}
```

This is a range predicate. The "operations wrap" claim is a separate contract (`ref-int32-arithmetic-overflow-v1`, planned) for arithmetic operations specifically.

## Why this exists

Many lift adapters need int32-range constraints. `@Min(-2147483648) @Max(2147483647)` (Bean Validation), `z.number().int().min(-2147483648).max(2147483647)` (zod), `Field(ge=-2147483648, le=2147483647)` (pydantic). All bridge to this reference.

A consumer's int32-range pre-condition matches; the bridge resolves; Tier 1 fires.

## Limitations

- **Two's complement assumption.** This reference assumes int32 with two's complement representation. C/C++ implementations using sign-magnitude or one's complement would need a separate reference.
- **Overflow behavior.** The reference says nothing about what happens on overflow during arithmetic operations. That's `ref-int32-arithmetic-overflow-v1` (proposed).
- **No bit-level semantics.** Bitwise operations on int32 (signed shift behaviors, etc.) are undefined behavior in C; well-defined in Rust; need a separate reference for the languages where they matter.

## Related references

- [`ref-uint32-arithmetic-v1.md`](ref-uint32-arithmetic-v1.md) — unsigned counterpart.
- `ref-int64-arithmetic-v1` (proposed).
- `ref-int32-arithmetic-overflow-v1` (proposed) — explicit wrapping arithmetic semantics.
- `ref-ieee754-arithmetic-v1` (proposed) — floating-point semantics.

## Read next

- [`README.md`](README.md).
- [`ref-parseInt-v1.md`](ref-parseInt-v1.md) — uses this reference internally.
