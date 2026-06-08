# ref-parseInt-v1

The canonical reference contract for ECMA-262-style integer parsing.

> **Status:** stub. Canonical IR not yet committed to `protocol/reference-contracts/`. CID below is a placeholder; the actual CID is computed from the committed canonical bytes.

## CID

`blake3-512:bafy...ref-parseInt-v1` *(placeholder; actual CID is BLAKE3-512 of the committed canonical IR bytes)*

## What it claims

> For any string `s` and integer `n`, if a function called as `parseInt(s)` returns `Some(n)`, then:
> - `n` is in the range `[INT_MIN, INT_MAX]` (i.e., representable as a 32-bit signed integer);
> - the canonical string representation of `n` is a substring of `s`;
> - the function is total: every input string yields either `Some(n)` for some integer `n` or `None`.

## Canonical IR (sketch)

```
{
  "kind": "contract",
  "name": "ref-parseInt-v1",
  "version": 1,
  "params": [
    {"name": "s", "sort": {"kind": "primitive", "name": "String"}}
  ],
  "return_sort": {"kind": "optional", "of": {"kind": "primitive", "name": "Int"}},
  "post": {
    "kind": "forall",
    "var": "n",
    "sort": {"kind": "primitive", "name": "Int"},
    "body": {
      "kind": "implies",
      "left": {
        "kind": "atomic", "predicate": "eq",
        "args": [{"kind": "var", "name": "result"}, {"kind": "ctor", "name": "Some", "args": [{"kind": "var", "name": "n"}]}]
      },
      "right": {
        "kind": "and",
        "left": {
          "kind": "and",
          "left": {"kind": "atomic", "predicate": "ge", "args": [{"kind": "var", "name": "n"}, {"kind": "const", "type": "Int", "value": -2147483648}]},
          "right": {"kind": "atomic", "predicate": "le", "args": [{"kind": "var", "name": "n"}, {"kind": "const", "type": "Int", "value": 2147483647}]}
        },
        "right": {
          "kind": "atomic", "predicate": "string_of_int_substring",
          "args": [{"kind": "var", "name": "n"}, {"kind": "var", "name": "s"}]
        }
      }
    }
  }
}
```

The full canonical IR lives at `protocol/reference-contracts/ref-parseInt-v1.json` (when committed). The CID is BLAKE3-512 of the JCS-canonicalized bytes of this file.

## Implementations that bridge to this reference

When implementations exist and bridge in, list them here:

- [TypeScript] zod's `z.string().pipe(z.coerce.number().int())` chain: *bridge planned in `sugar-lift-zod` v0.4*
- [Rust] `std::str::parse::<i32>()`: *bridge planned in `sugar-lift-contracts` for the contracts crate's `parseInt` model*
- [Python] `int(...)` builtin: *bridge planned in `sugar-lift-py-stdlib`*
- [Java] `Integer.parseInt(String)`: *bridge planned in `sugar-lift-java-jdk`*
- [Go] `strconv.Atoi`: *bridge planned in `sugar-lift-go-stdlib`*

A consumer in any language whose pre-condition matches `ref-parseInt-v1` discharges at Tier 1 against any implementation bridging in.

## Limitations

- **Locale.** The reference does not capture locale-specific digit characters. Implementations that accept Devanagari digits or fullwidth ASCII have semantics outside this reference; a separate `ref-parseInt-locale-v1` could capture that.
- **Whitespace.** The reference does not specify leading/trailing whitespace handling. Implementations that strip whitespace before parsing have semantics outside this reference; bridge to `ref-parseInt-trimmed-v1` (proposed) for that.
- **Base prefixes.** The reference does not specify hex / oct / binary prefix handling (`0x`, `0o`, `0b`). Implementations that recognize prefixes have semantics outside this reference.
- **Error mode.** The reference uses `Option`; implementations that throw on parse failure (Java, Python in some modes) need an additional adapter to map exceptions to `None`.

For the most precise contract for a specific implementation, use a per-implementation contract memento alongside the bridge to the reference.

## Why this reference exists

Integer parsing is one of the most common cross-language call sites. Validation, coercion, and parsing of user input span every host language. A canonical reference for the basic semantics ("result in int32 range, canonical representation matches input substring") covers the majority of use cases.

More precise references (locale-aware, whitespace-aware, prefix-aware) can be added as separate references. This v1 is the conservative core.

## Related references

- [`ref-parseFloat-v1.md`](ref-parseFloat-v1.md): float parsing.
- `ref-parseInt-trimmed-v1` (proposed): int parsing with whitespace handling.
- `ref-parseInt-prefixes-v1` (proposed): int parsing with hex/oct/binary prefixes.
- `ref-parseInt-locale-v1` (proposed): locale-aware digits.

## Read next

- [`README.md`](README.md): what reference contracts are.
- [`../explanation/cross-domain-verification.md`](../explanation/cross-domain-verification.md): the mechanism this reference enables.
- [`../tutorials/polyglot-stack.md`](../tutorials/polyglot-stack.md): the worked demo using this reference.
