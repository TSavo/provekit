// Fixture: a representative slice of liftable proptest blocks. Used by
// integration tests in provekit-lift/tests/. NOT compiled (this file
// has no Cargo target); the lift adapter parses it as text via syn.

proptest! {
    #[test]
    fn nonneg(x: i64) {
        prop_assert!(x >= 0);
    }

    #[test]
    fn answer_is_42(x: i64) {
        prop_assert_eq!(x, 42);
    }

    #[test]
    fn cid_len_constant(bytes: i64) {
        prop_assert_eq!(compute_cid(bytes), 139);
    }

    #[test]
    fn upper_bound(x: i64) {
        prop_assert!(x < 1000000);
    }

    #[test]
    fn not_equal_to_zero(x: i64) {
        prop_assert_ne!(x, 0);
    }

    #[test]
    fn string_is_hello(s: String) {
        prop_assert_eq!(s, "hello");
    }
}

proptest! {
    #[test]
    fn another_block(y: i64) {
        prop_assert!(y > -10);
    }
}
