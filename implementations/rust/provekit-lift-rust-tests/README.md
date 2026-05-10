# provekit-lift-rust-tests

Lift unit tests as point-specific behavior witnesses.

> A unit test is a point-specific predicate: "at this input, this output." A property test is a universal predicate: "forall input, this property." Both are content-addressable behavior witnesses. ProvekIt lifts both.
>
> Every passing test in your codebase becomes a content-addressed signed contract memento. Test authors don't need to write contracts; they already wrote the contracts. We just promote them.

## What it does

Walks the `syn` AST of a Rust source file looking for `#[test]` and
`#[tokio::test]` functions. Inside each function body, assertion macros
(`assert_eq!`, `assert_ne!`, `assert!`, `assert_matches!`) become
`ContractDecl`s only when the asserted value can be tied to a producer
callsite. Let-bound observations such as `let r = foo(5); assert_eq!(r, 10);`
are attached to the `foo(5)` callsite and substitute the binding into the
lifted formula.

Naming: `<callee>@<file>:<line>:<col>`. Multi-call assertions produce one
contract per callsite. Assertions with no identifiable callsite skip with a
warning.

## v0.5 grammar (whitelist)

For assertion expressions we accept identifiers, integer/string/byte-string/bool
literals, paths, calls, method calls, references, casts, parens, arrays,
tuples, `vec![]`, and supported binary/unary operand shapes. Calls are lifted
as constructors in the formula; method calls use the same one-level UFCS
flattening as the adapter code.

Anything outside the whitelist, or any liftable assertion with no identifiable
producer callsite, is SKIPPED with a warning. Honest skips beat polluted
lattices.

`assert_matches!` is recognized only when the pattern is `Ok(<lit>)` /
`Err(<lit>)` / `Some(<lit>)` / `None` / a single bare ctor over literals;
deeper structural matches skip with a warning.

## Why no quantifiers

Unit tests use concrete inputs, so the lifted formula is a closed atomic
formula. No `forall`. The point IS that it's point-specific.
