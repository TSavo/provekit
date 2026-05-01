// Fixture: a representative slice of liftable #[quickcheck] property
// functions. NOT compiled (this file has no Cargo target); the lift
// adapter parses it as text via syn.

#[quickcheck]
fn qc_eq_self(a: i64) -> bool {
    a == a
}

#[quickcheck]
fn qc_nonneg(a: i64) -> bool {
    a >= -9223372036854775807
}

#[quickcheck]
fn qc_not_max(a: i64) -> bool {
    a != 9223372036854775807
}

#[quickcheck::quickcheck]
fn qc_str_hello(s: String) -> bool {
    s == "hello"
}

// Deliberately skipped pattern: TestResult-returning property is in the
// v0 skip list and emits a structured warning.
#[quickcheck]
fn qc_returns_test_result(a: i64) -> TestResult {
    TestResult::passed()
}
