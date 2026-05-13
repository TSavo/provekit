# Writing a lift adapter, step 3: emit canonical IR

The walker (step 2) extracted recognized annotations. This step maps each annotation to a canonical IR formula. The canonical IR is the protocol's lingua franca; if your adapter produces correct canonical IR, the rest is automatic.

This is the load-bearing step semantically. Step 2 is mechanical (walk an AST); this step is where you commit to a precise meaning for each annotation.

## The IR primitives you have

From the CDDL grammar at `protocol/provekit-ir.cddl`:

- **Term**: variables (`var(s)`), constants (`const(0)`), constructors (`pair(x, y)`), lambdas, let-bindings.
- **Formula**: atomic predicates (`atomic("eq", [x, 0])`), connectives (`and(p, q)`, `or(p, q)`, `not(p)`, `implies(p, q)`), quantifiers (`forall(x, sort, body)`, `exists(x, sort, body)`), choice.
- **Sort**: primitive types (`Int`, `String`, `Bool`, `Real`).

Any annotation in any source library lifts to a Formula over Terms of Sorts. Your job is to find the right canonical encoding for each annotation.

## Worked examples

### `@NotNull` (Bean Validation)

Semantics: the annotated field must not be null.

Canonical IR (assuming the field is `email: String` on parameter `x`):

```
forall x: String.
  x != null
```

In structured form:

```json
{
  "kind": "forall",
  "var": "x",
  "sort": {"kind": "primitive", "name": "String"},
  "body": {
    "kind": "atomic",
    "predicate": "not_null",
    "args": [{"kind": "var", "name": "x"}]
  }
}
```

Note: the canonical predicate name is `not_null`, not `isNotNull` or `nonNull` or `notNull`. Predicate names are part of the canonical IR vocabulary; consult [`docs/reference/ir/primitives.md`](../../reference/ir/primitives.md) (when written) for the full list.

### `@Min(0) @Max(150)` (Bean Validation, on `int age`)

Semantics: age >= 0 and age <= 150.

Canonical IR:

```
forall age: Int.
  (age >= 0) and (age <= 150)
```

Crucially, this lifts to **the same canonical IR** as JML's `//@ requires age >= 0 && age <= 150` and Spring Web's `@Min(0) @Max(150) int age`. That's the cross-domain equivalence claim: equivalent constraints in different annotation idioms produce byte-identical IR.

### `z.string().email()` (zod, on field `email`)

Semantics: email is a string matching the email regex.

Canonical IR:

```
forall email: String.
  matches_pattern(email, "^[^@]+@[^@]+\\.[^@]+$")
```

Note: the regex is the canonical email regex from RFC 5322 (or a documented subset). If your adapter chooses a different regex, the canonical IR diverges from the Pydantic / Bean Validation `@Email` adapters. To make `z.string().email()` lift to the same canonical IR as `@Email` and `EmailStr`, all three adapters must use the same canonical regex.

This is why **shared canonical predicates and shared canonical regexes matter**. They are part of the protocol's vocabulary, maintained per-domain, and consulted by every adapter.

### `prop_assume!(x > 0)` + `prop_assert!(x.parse::<i32>().is_ok())` (proptest)

Semantics: assuming x > 0, the parsed result is Ok.

Canonical IR (a contract: pre x > 0, post result is_ok):

```
{
  "kind": "contract",
  "pre": {
    "kind": "atomic", "predicate": "gt",
    "args": [{"kind": "var", "name": "x"}, {"kind": "const", "value": 0}]
  },
  "post": {
    "kind": "atomic", "predicate": "is_ok",
    "args": [{"kind": "atomic", "predicate": "parse_int_result",
              "args": [{"kind": "var", "name": "x"}]}]
  }
}
```

`proptest` block lifting is more involved than per-annotation lifting because each block has assumes + asserts; the adapter folds them into a single contract per block.

## Canonical predicate vocabulary

The IR's canonical predicates are documented per domain. A starter set:

- **Comparison**: `eq`, `ne`, `lt`, `le`, `gt`, `ge`.
- **Numeric**: `is_int`, `is_finite`, `is_nan`, `is_positive`, `is_negative`, `is_zero`.
- **String**: `length_eq`, `length_ge`, `length_le`, `matches_pattern`, `starts_with`, `ends_with`, `contains`.
- **Collection**: `length`, `member`, `subset`, `forall_in`, `exists_in`.
- **Null/Optional**: `not_null`, `is_some`, `is_none`.
- **Domain-specific**: `is_email_format`, `is_url_format`, `is_uuid_format`, `is_ip_address`, etc.

When you encounter an annotation whose semantics aren't covered by an existing canonical predicate, you have two choices:

1. **Map to a composition of existing predicates.** Often possible. `@Length(min=5, max=10)` becomes `length_ge(x, 5) and length_le(x, 10)`.
2. **Propose a new canonical predicate.** Requires a spec change. See [docs/contributing/proposing-a-spec-change.md](../proposing-a-spec-change.md) (when written).

## Sort inference

Each variable in a quantifier needs a Sort. For statically-typed languages, the Sort comes from the type system. For dynamically-typed languages, infer from type hints / type annotations / runtime types.

If you cannot infer a Sort, use `Sort.Any` and accept that the verifier will treat the bound variable as untyped. This reduces what the verifier can prove. Adapters should only fall back to `Sort.Any` when no better option exists.

## Cross-adapter equivalence

The cross-domain claim is that `@NotNull` (Bean Validation) and `//@ requires email != null` (JML) produce the same canonical IR. Achieving this requires the adapters to:

1. Use the same canonical predicate names (`not_null`, not each adapter's preferred phrasing).
2. Use the same Sort canonicalization (`String`, not each adapter's local string sort).
3. Use the same variable-naming convention for the bound parameter (typically the field/parameter name).
4. Produce the same JCS bytes for the encoded IR.

Conformance fixtures cover the third and fourth. The first and second are conventions enforced by adapter authors reading [`docs/reference/ir/primitives.md`](../../reference/ir/primitives.md) (when written) before writing the adapter.

## What to do with annotations whose semantics differ from any existing canonical predicate

This happens. Some libraries have unique constraints. Two valid responses:

1. **Lift to a composition** if possible. Often a unique constraint is expressible as `and` / `or` of existing predicates plus standard comparisons.
2. **Skip the annotation** for now. Better than lifting incorrectly. Document the gap in the adapter's README.

Adding a new canonical predicate is a spec change, not an adapter change. Treat it as such.

## Test the canonicalization layer

Given a structured intermediate from step 2, your canonicalization layer produces canonical IR JSON bytes. Test:

1. **Pure-input → expected-output tests.** For each annotation kind, hand-write the expected canonical IR; verify your code produces the same bytes.
2. **Cross-adapter equivalence tests.** If your adapter targets a library whose annotations should equiv to another library's, write tests asserting both produce identical bytes. This is what made the Java kit's `@NotNull` ↔ `//@ requires x != null` ↔ `@RequestParam(required=true)` claim auditable.
3. **Property-based tests.** For complex constraint compositions, generate random constraints and verify the canonical IR round-trips.

## Read next

- [04-conformance-test.md](04-conformance-test.md): the fixture-based conformance gate.
- [docs/reference/ir/](../../reference/ir/) (when written): IR primitives and canonical predicates reference.
