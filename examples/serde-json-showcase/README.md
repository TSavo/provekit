# serde_json Showcase

This showcase adds a real Rust library logo: `serde_json`.

The GOOD suite carries exact rows from `serde_json 1.0.150` vendor tests:

- `tests/test.rs::test_write_null`
- `tests/test.rs::test_write_u64`
- `tests/test.rs::test_write_str`
- `tests/test.rs::test_write_bool`

The showcase deliberately stays inside point-wise exact assertions. Residuals
include tolerance-free but non-flat helper loops, error-string rows that require
format-macro modeling, map ordering rows behind feature configuration, and
nonfinite-float rows.

The BAD suite is an explicit contradiction twin over the same vendor value:
`serde_json::to_string(&true).unwrap()` is asserted equal to both `"true"` and
`"false"`. Consistency must refuse it, and the cargo-test witness package must
also refuse because the test really fails.
