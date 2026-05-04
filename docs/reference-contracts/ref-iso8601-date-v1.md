# ref-iso8601-date-v1

The canonical reference contract for ISO 8601 date format.

> **Status:** stub.

## CID

`blake3-512:bafy...ref-iso8601-date-v1` *(placeholder)*

## What it claims

> A string `s` matches the ISO 8601 date format predicate if and only if `s` is in `YYYY-MM-DD` form with valid month and day for the given year.

Canonical regex (date-only, no time, no timezone):

```
^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])$
```

## Canonical IR (sketch)

```
{
  "kind": "contract",
  "name": "ref-iso8601-date-v1",
  "version": 1,
  "params": [
    {"name": "s", "sort": {"kind": "primitive", "name": "String"}}
  ],
  "post": {
    "kind": "atomic",
    "predicate": "matches_pattern",
    "args": [
      {"kind": "var", "name": "s"},
      {"kind": "const", "type": "String", "value": "^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])$"}
    ]
  }
}
```

## Limitations

- **Date only.** No time component. `2025-04-23T14:30:00Z` does not match this reference; use `ref-iso8601-datetime-v1` (proposed).
- **Day validity.** The regex catches `2025-02-30` because `30` matches `[12][0-9]`, but February 30 doesn't exist. The reference includes a syntactic check; semantic-validity (Feb only has 28-29 days; April only 30) requires a stronger predicate (`ref-iso8601-date-strict-v1`, proposed).
- **No leap second handling.**
- **No timezone information.**

## Implementations that bridge to this reference

- [TypeScript] zod's `z.string().date()`: *bridge planned*
- [Python] pydantic's `date` field: *bridge planned*
- [Java] Bean Validation's `@PastOrPresent` on `LocalDate`: *bridge planned*

## Read next

- [`README.md`](README.md).
- [`ref-uuid-v1.md`](ref-uuid-v1.md): sibling format reference.
