# Lift adoption paths

ProvekIt does not compete with the annotation libraries already deployed in the wild. It sits beneath them. Whatever library a codebase already uses, the lift adapter promotes those annotations to content-addressed signed contract mementos. This document is the per-source-library adoption guide.

The pattern is uniform across host languages: a lift adapter walks the source library's idiom, emits canonical IR, mints a signed contract memento, and publishes. Authoring stays where the developer already is; only verification moves underneath.

## The shipping adapters (v1.1)

### Rust: `proptest`

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn divide_never_panics(a: i64, b: i64) {
        prop_assume!(b != 0);
        let result = divide(a, b);
        prop_assert!(result.is_finite());
    }
}
```

`provekit-lift-proptest` walks `proptest!` blocks, recognizes the universal-quantification idiom, and emits a contract memento whose `pre` is the conjunction of `prop_assume!` clauses and whose `post` is the conjunction of `prop_assert!` clauses. The memento is bridged to the function under test (`divide` in this example) via a bridge memento in the same `.proof`.

After `provekit mint --project <workspace-root>`, the resulting catalog carries the universal property as a content-addressed signed contract. Consumers downstream verify against the contract without re-running the property test.

**Coverage today:** `prop_assume!`, `prop_assert!`, `prop_assert_eq!`, `prop_assert_ne!`, basic arithmetic and comparison predicates. Strategy expressions are recognized but not all are lifted.

### Rust: `contracts`

```rust
use contracts::*;

#[requires(b != 0)]
#[ensures(ret.is_finite())]
fn divide(a: f64, b: f64) -> f64 {
    a / b
}
```

`provekit-lift-contracts` walks `#[requires(...)]`, `#[ensures(...)]`, and `#[invariant(...)]` macros. Each annotation becomes one term of the contract memento's `pre`, `post`, or `inv`. The bridge memento maps the function symbol to the contract CID.

**Coverage today:** `requires`, `ensures`, `invariant`. Old/new variable references in `ensures` are partially supported; complex existential quantification routes through Tier 3 of the handshake.

## Planned for v1.2

Each adapter below has the same shape: walk the source library's annotation idiom, emit canonical IR, mint a signed contract memento. The protocol bytes are uniform regardless of the source library's surface syntax.

### Rust: `kani`

```rust
#[kani::proof]
fn divide_proof() {
    let a: f64 = kani::any();
    let b: f64 = kani::any();
    kani::assume(b != 0.0);
    let r = divide(a, b);
    kani::assert(r.is_finite(), "result is finite");
}
```

The kani adapter walks `#[kani::proof]` functions, treating `kani::assume` clauses as the `pre` and `kani::assert` clauses as the `post`. Universal quantification is introduced over `kani::any()` calls.

### Rust: `prusti`

```rust
#[prusti_contracts::requires(b != 0)]
#[prusti_contracts::ensures(result.is_finite())]
fn divide(a: f64, b: f64) -> f64 { a / b }
```

The prusti adapter is a near-clone of the contracts adapter; the macro names differ but the lift logic is the same.

### Rust: `creusot`, `flux`

Under evaluation. Both libraries express richer logical fragments than the IR's current admissible theories; some annotations will lift cleanly, others will route through Tier 3 of the handshake or fall outside the protocol's scope until the IR extension protocol absorbs the relevant theories.

### TypeScript: `zod`

```ts
import { z } from 'zod';

const UserSchema = z.object({
  email: z.string().email(),
  age: z.number().int().min(0).max(150),
});
```

The zod adapter walks schema definitions, recognizes the validator combinators (`.email()`, `.int()`, `.min()`, `.max()`), and emits a contract memento whose `post` is the conjunction of validator predicates. A bridge memento maps the schema symbol (`UserSchema`) to the contract CID.

### TypeScript: `class-validator`

```ts
import { IsEmail, IsInt, Min, Max } from 'class-validator';

class User {
  @IsEmail() email: string;
  @IsInt() @Min(0) @Max(150) age: number;
}
```

The class-validator adapter walks decorator-annotated class fields, recognizes the validator decorator set, and emits a per-class contract memento. The bridge memento maps the class symbol to the contract CID.

### TypeScript: `fast-check`

```ts
import fc from 'fast-check';

fc.assert(fc.property(
  fc.integer(), fc.integer(),
  (a, b) => b !== 0 ? Number.isFinite(a / b) : true
));
```

The fast-check adapter walks `fc.assert(fc.property(...))` blocks, treating the property's input arbitraries as universal quantification and the property's body as the `post`.

### Python: `pydantic`

```python
from pydantic import BaseModel, EmailStr, Field

class User(BaseModel):
    email: EmailStr
    age: int = Field(ge=0, le=150)
```

The pydantic adapter walks model class field annotations and constraint metadata, recognizes the validator types and `Field` constraints, and emits a contract memento whose `post` is the conjunction of validator predicates.

### Python: `deal`

```python
import deal

@deal.pre(lambda a, b: b != 0)
@deal.post(lambda result: result == result)  # not NaN
def divide(a: float, b: float) -> float:
    return a / b
```

The deal adapter walks `@deal.pre`, `@deal.post`, `@deal.raises`, and `@deal.has` decorators, treating each lambda as a contract clause.

### Python: `hypothesis`

```python
from hypothesis import given, strategies as st, assume

@given(a=st.integers(), b=st.integers())
def test_divide(a, b):
    assume(b != 0)
    assert math.isfinite(divide(a, b))
```

The hypothesis adapter walks `@given(...)` test functions, treating `assume(...)` as the `pre`, `strategies` as the universal quantification, and `assert` clauses as the `post`. Shape is similar to the proptest adapter.

### Java: Bean Validation (`jakarta.validation`, `javax.validation`)

```java
public class User {
    @NotNull @Email
    private String email;

    @Min(0) @Max(150)
    private int age;
}
```

The Bean Validation adapter walks `@NotNull`, `@Email`, `@Min`, `@Max`, `@Pattern`, `@Size`, and the rest of the standard validator annotations. Per-class contract mementos are emitted with the validator predicates as the `post`.

### Java: JML, Cofoja

Both libraries express full pre/post/invariant contracts in Java. The JML adapter walks `//@ requires`, `//@ ensures`, and `//@ invariant` comment-block annotations. The Cofoja adapter walks `@Requires` and `@Ensures` annotations. Both produce contract mementos identical in shape to the Rust `contracts` adapter's output.

### Go: `go-playground/validator`

```go
type User struct {
    Email string `validate:"required,email"`
    Age   int    `validate:"gte=0,lte=150"`
}
```

The validator adapter walks struct tag metadata, recognizes the standard validator tags (`required`, `email`, `gte`, `lte`, `min`, `max`, `oneof`), and emits a contract memento per struct.

### C++: contract attributes

```cpp
double divide(double a, double b)
    [[expects: b != 0]]
    [[ensures r: std::isfinite(r)]]
{
    return a / b;
}
```

The C++ adapter walks `[[expects:]]` and `[[ensures:]]` attributes (C++26 contract syntax), treating them as `pre` and `post` clauses. Adapters for `assert.h` patterns and Boost.Contract are under evaluation.

## How to read this list

Each adapter is real engineering. The shipping adapters in v1.1 are `provekit-lift-proptest` and `provekit-lift-contracts` for Rust. Everything else is on the v1.2 roadmap or under evaluation.

The pattern's uniformity is the load-bearing claim. Whatever annotation library a codebase already uses, the lift adapter's job is the same: walk the idiom, emit canonical IR, mint a signed contract memento. The protocol bytes the adapter produces are identical regardless of which source library the input came from. Two contract mementos, one from `proptest` and one from `pydantic`, expressing the same proposition, share a CID.

This is the cross-language conformance property in action. A Rust consumer of a Python library and a Python consumer of a Rust library see the same Tier-1 hash-discharge fraction, because the IR is language-agnostic and the canonicalization pipeline is deterministic.

## What lifting does NOT do

Lifting does not change what the source library expresses. If `proptest` covers a property over the input domain via random sampling, the lifted contract memento covers the same property in the same logical fragment; it does not magically extend coverage to inputs the original library could not reach. The contract memento is a portable, signed, content-addressed encoding of the source library's existing claim.

Lifting does not validate that the source library's claim is correct. The contract memento records "this annotation said this"; if the annotation was wrong (a `proptest!` block that misstates the property, a `pydantic` constraint that does not match the runtime behavior), the lifted memento carries the same defect. ProvekIt's protocol guarantees signature unforgeability and hash determinism; it does not guarantee the source library's correctness.

Lifting does not require the source library at runtime. Once the `.proof` is produced, the consumer's verifier reads the catalog and runs the handshake; the original `proptest` runner, `contracts` macro expansion, or `pydantic` validator is not needed. The `.proof` is portable across machines that do not have the source library installed.

## How to write a new adapter

The interface is small. A lift adapter implements:

1. A walker over the source library's idiom (typically using `syn` for Rust, the TypeScript Compiler API for TS, `ast` for Python, JavaParser for Java).
2. A translator from idiom to IR (using the host-language IR library: `provekit-ir-symbolic`).
3. A bridge emitter that maps each lifted contract to its source-language symbol.

The output is a sequence of `(contractMemento, bridgeMemento)` pairs. The driver (`provekit mint --project <workspace-root>` for Rust, equivalents for other languages) takes care of canonicalization, hashing, signing, and writing the `.proof`.

If you want to write an adapter for a library not on this list, the per-language kit standard at CID `blake3-512:7d3e72d58c87864eea2b7b330096d2cc4591292c1905baa447d4f74b8d80327521e284fc37f874fae80ba8f170a2456aed27c37215ee8752f8fd57e2d60b0f88` defines the contract every adapter implements. Reach out via the project repository; adapter contributions are explicitly in scope.
