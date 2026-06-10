#[cfg(test)]
mod tests {
    use itertools::Itertools;

    #[test]
    fn join_many_exact_row() {
        // Vendor source: itertools 0.14.0 tests/test_std.rs::join.
        // Exact row: [1, 2, 3].iter().join(", ") == "1, 2, 3".
        let many = [1, 2, 3];
        assert_eq!(many.iter().join(", "), "1, 2, 3");
    }

    #[test]
    fn join_one_exact_row() {
        // Vendor source: itertools 0.14.0 tests/test_std.rs::join.
        // Exact row: [1].iter().join(", ") == "1".
        let one = [1];
        assert_eq!(one.iter().join(", "), "1");
    }

    #[test]
    fn join_empty_exact_row() {
        // Vendor source: itertools 0.14.0 tests/test_std.rs::join.
        // Exact row: empty vec join yields empty string.
        let none: Vec<i32> = vec![];
        assert_eq!(none.iter().join(", "), "");
    }

    #[test]
    fn all_equal_true_exact_row() {
        // Vendor source: itertools 0.14.0 tests/test_std.rs::all_equal.
        // Exact row: "AAAAAAA".chars().all_equal() == true.
        assert!("AAAAAAA".chars().all_equal());
    }

    #[test]
    fn all_equal_false_exact_row() {
        // Vendor source: itertools 0.14.0 tests/test_std.rs::all_equal.
        // Exact row: "AABBCCC".chars().all_equal() == false.
        assert!(!"AABBCCC".chars().all_equal());
    }

    #[test]
    fn all_unique_true_exact_row() {
        // Vendor source: itertools 0.14.0 tests/test_std.rs::all_unique.
        // Exact row: "ABCDEFGH".chars().all_unique() == true.
        assert!("ABCDEFGH".chars().all_unique());
    }

    #[test]
    fn all_unique_false_exact_row() {
        // Vendor source: itertools 0.14.0 tests/test_std.rs::all_unique.
        // Exact row: "ABCDEFGA".chars().all_unique() == false (duplicate 'A').
        assert!(!"ABCDEFGA".chars().all_unique());
    }

    #[test]
    fn kmerge_empty_size_hint_exact_row() {
        // Vendor source: itertools 0.14.0 tests/test_std.rs::kmerge_empty_size_hint.
        // Exact row: kmerge of five empty ranges has size_hint (0, Some(0)).
        let its = (0..5).map(|_| (0..0));
        assert_eq!(its.kmerge().size_hint(), (0, Some(0)));
    }

    #[test]
    fn sorted_by_cached_key_ncalls_4_exact_row() {
        // Vendor source: itertools 0.14.0 tests/test_std.rs::sorted_by_cached_key.
        // Exact row: key function called exactly 4 times for 4-element slice.
        let mut ncalls = 0;
        let sorted = [3, 4, 1, 2].iter().cloned().sorted_by_cached_key(|&x| {
            ncalls += 1;
            x.to_string()
        });
        let collected: Vec<_> = sorted.collect();
        assert_eq!(collected, vec![1, 2, 3, 4]);
        assert_eq!(ncalls, 4);
    }
}
