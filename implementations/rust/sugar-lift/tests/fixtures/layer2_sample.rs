// Fixture: Layer 2 lift patterns. Used by integration tests in
// sugar-lift/tests/. NOT compiled (this file has no Cargo target);
// the lift adapter parses it as text via syn.
//
// Layer 2 catches structural patterns Layer 0 cannot:
//   - Pattern 1: bounded loop as universal quantifier
//   - Pattern 2: inlined helper functions
//   - Pattern 3: multi-assertion characterization conjunction
//
// One test per file deliberately exercises a SKIP path so the
// integration test can assert the structured warning surface.

// ---- Pattern 1: bounded loops ----------------------------------------

#[test]
fn squares_are_nonneg() {
    for x in 0..100 {
        assert!(x >= 0);
    }
}

#[test]
fn divmod_in_range() {
    for x in 1..=50 {
        assert!(x > 0);
    }
}

#[test]
fn small_window() {
    for x in 0..16 {
        assert!(x < 100);
    }
}

// ---- Pattern 2: helper inlining --------------------------------------

fn assert_is_42(x: i64) {
    assert_eq!(x, 42);
}

fn assert_in_range(y: i64) {
    assert!(y >= 0);
}

#[test]
fn many_42s() {
    assert_is_42(42);
    assert_is_42(42);
    assert_is_42(42);
}

#[test]
fn ranges_ok() {
    assert_in_range(0);
    assert_in_range(1);
}

// ---- Pattern 3: multi-assertion characterization ---------------------

#[test]
fn parse_int_characterization() {
    assert_eq!(parse_int("0"), 0);
    assert_eq!(parse_int("42"), 42);
    assert_ne!(parse_int("99"), 0);
}

// ---- Pattern 1 deliberately-skipped: nested loop (Layer 2.5) ---------

#[test]
fn nested_loop_skipped() {
    for x in 0..10 {
        for y in 0..10 {
            assert!(x >= 0);
        }
    }
}
