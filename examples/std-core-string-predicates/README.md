# std-core-string-predicates

This showcase closes the narrow std/core string-predicate gap named by the
main `std-core-showcase`: point-wise string, ASCII-char, and literal iterator
claims from Rust's own tests or doctest source now lift as `#euf#` contract
rows and verify through z3 string theory or the corresponding arithmetic range
checks for literal byte slices.

Claimed GOOD slice:

- `library/alloctests/tests/str.rs`: `test_starts_with`, `test_ends_with`,
  `test_contains`, `test_contains_char`, and the literal `str::len` row from
  `test_join_for_different_lengths_with_long_separator`.
- `library/coretests/tests/ascii.rs`: the literal `str::is_ascii` rows and the
  literal `chars().all/.any` and `bytes().is_ascii` rows from `test_is_ascii`.
- `library/core/src/char/methods.rs`: documented doctest point examples for
  `char::is_ascii` and `char::is_ascii_alphabetic`.

The script also builds a BAD negative-control twin by contradicting one vendor
point assertion. That BAD file is not a vendor spec claim; it exists only to
prove the string-theory row is refused when z3 sees an UNSAT conjunction.

Conservative residuals:

- Unicode `char::is_alphabetic` is not lifted. Rust delegates that to the
  Unicode Character Database; z3 string theory does not encode Rust's Unicode
  `Alphabetic` table here.
- Non-literal receivers such as `let data = "..."; assert!(data.contains(...))`
  remain residual until the lifter tracks bindings.
- Iterator predicates that are not literal `chars()/bytes()` walks remain
  residual until the lifter tracks arbitrary sources and closure bodies.
