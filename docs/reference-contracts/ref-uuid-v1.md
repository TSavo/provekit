# ref-uuid-v1

The canonical reference contract for RFC 4122 UUID format.

> **Status:** stub.

## CID

`blake3-512:bafy...ref-uuid-v1` *(placeholder; computed from committed canonical IR)*

## What it claims

> A string `s` matches the UUID format predicate if and only if `s` is a 36-character lowercase string in the `8-4-4-4-12` hex pattern with hyphens, where the version digit and variant bits are RFC 4122-compliant (versions 1-5 supported).

Canonical regex:

```
^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$
```

## Canonical IR (sketch)

```
{
  "kind": "contract",
  "name": "ref-uuid-v1",
  "version": 1,
  "params": [
    {"name": "s", "sort": {"kind": "primitive", "name": "String"}}
  ],
  "post": {
    "kind": "atomic",
    "predicate": "matches_pattern",
    "args": [
      {"kind": "var", "name": "s"},
      {"kind": "const", "type": "String", "value": "^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"}
    ]
  }
}
```

## Implementations that bridge to this reference

- [TypeScript] zod's `z.string().uuid()` — *bridge planned*
- [Python] pydantic's `UUID4` (and friends) — *bridge planned*
- [Rust] `uuid::Uuid` parsing — *bridge planned*
- [Java] `java.util.UUID.fromString` — *bridge planned*

## Limitations

- **Lowercase only.** The canonical regex requires lowercase hex digits. Implementations that accept uppercase need a separate reference (`ref-uuid-case-insensitive-v1`, proposed) or normalize before validating.
- **Versions 1-5.** Versions 6, 7, 8 (newer UUID versions) require `ref-uuid-v2` (proposed).
- **Compact form.** UUIDs without hyphens (the 32-char compact form) need a separate reference.

## Read next

- [`README.md`](README.md).
- [`ref-email-format-v1.md`](ref-email-format-v1.md) — sibling reference.
