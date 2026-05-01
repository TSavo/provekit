# provekit-lift-rust-tests

Lift unit tests as point-specific behavior witnesses.

> A unit test is a point-specific predicate: "at this input, this output." A property test is a universal predicate: "forall input, this property." Both are content-addressable behavior witnesses. ProvekIt lifts both.
>
> Every passing test in your codebase becomes a content-addressed signed contract memento. Test authors don't need to write contracts; they already wrote the contracts. We just promote them.

## What it does

Walks the `syn` AST of a Rust source file looking for `#[test]` and
`#[tokio::test]` functions. Inside each function body, every assertion macro
invocation (`assert_eq!`, `assert_ne!`, `assert!`, `assert_matches!`) becomes
its own `ContractDecl` whose `inv` field is the lifted atomic predicate.

Naming: `<test_function_name>::<assertion_index>` (zero-indexed, in source
order). Three asserts in one test produce three contracts.

## v0 grammar (whitelist)

For each side of an assertion expression we accept:

- identifier (lifted to `Var`)
- integer literal (lifted to `Const(Int)`)
- string literal (lifted to `Const(String)`)
- single-arg call (lifted to `Ctor` with one arg)

Anything else (method calls, field access, indexing, multi-arg non-ctor
calls, arithmetic in the test body, randomness, filesystem ops) is SKIPPED
with a warning. Honest skips beat polluted lattices.

`assert_matches!` is recognized only when the pattern is `Ok(<lit>)` /
`Err(<lit>)` / `Some(<lit>)` / `None` / a single bare ctor over literals;
deeper structural matches skip with a warning.

## Why no quantifiers

Unit tests use concrete inputs, so the lifted formula is a closed atomic
formula. No `forall`. The point IS that it's point-specific.
