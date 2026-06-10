# regex Showcase

This showcase adds a real Rust library logo: `regex`.

The GOOD suite carries exact point-wise rows from `regex 1.12.4` vendor tests:

- `tests/regression.rs::invalid_regexes_no_crash`
- `tests/regression.rs::regression_invalid_repetition_expr`
- `tests/regression.rs::regression_invalid_flags_expression`
- `tests/regression_fuzz.rs::fail_branch_prevents_match`

The BAD suite is an explicit contradiction twin over the same valid-regex
predicate from `regression_invalid_flags_expression`: the same
`Regex::new("(((?x)))").is_ok()` call is asserted true and false. Consistency
must refuse it, and the cargo-test witness package must also refuse because the
test really fails.

Residuals are regex's data-driven TOML suite, iterator collection rows,
capture indexing rows, replacement macro rows, long-running ignored fuzz rows,
and feature-gated Unicode variants. Those are real vendor tests but outside the
current flat point-wise lift shape.
