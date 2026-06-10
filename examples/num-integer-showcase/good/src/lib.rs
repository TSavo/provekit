#[cfg(test)]
mod tests {
    use num_integer::Integer;

    #[test]
    fn gcd_12_8_exact_row() {
        // Vendor source: num-integer 0.1.46 doc-examples.
        // Exact row: gcd(12, 8) == 4.
        assert_eq!(num_integer::gcd(12i32, 8i32), 4i32);
    }

    #[test]
    fn lcm_7_3_exact_row() {
        // Vendor source: num-integer 0.1.46 doc-examples.
        // Exact row: lcm(7, 3) == 21.
        assert_eq!(num_integer::lcm(7i32, 3i32), 21i32);
    }

    #[test]
    fn gcd_zero_exact_row() {
        // Vendor source: num-integer 0.1.46 doc-examples.
        // Exact row: gcd(0, 5) == 5 (identity element for gcd).
        assert_eq!(num_integer::gcd(0i32, 5i32), 5i32);
    }

    #[test]
    fn div_floor_pos_exact_row() {
        // Vendor source: num-integer 0.1.46 Integer trait doc-examples.
        // Exact row: div_floor(7, 3) == 2 (floor division, positive inputs).
        assert_eq!(Integer::div_floor(&7i32, &3i32), 2i32);
    }

    #[test]
    fn mod_floor_pos_exact_row() {
        // Vendor source: num-integer 0.1.46 Integer trait doc-examples.
        // Exact row: mod_floor(7, 3) == 1.
        assert_eq!(Integer::mod_floor(&7i32, &3i32), 1i32);
    }

    #[test]
    fn div_floor_neg_exact_row() {
        // Vendor source: num-integer 0.1.46 Integer trait doc-examples.
        // Exact row: div_floor(-7, 3) == -3 (floored, not truncated).
        // Distinguishing behavior: truncating division would give -2.
        assert_eq!(Integer::div_floor(&-7i32, &3i32), -3i32);
    }

    #[test]
    fn mod_floor_neg_exact_row() {
        // Vendor source: num-integer 0.1.46 Integer trait doc-examples.
        // Exact row: mod_floor(-7, 3) == 2 (always non-negative when divisor positive).
        assert_eq!(Integer::mod_floor(&-7i32, &3i32), 2i32);
    }

    #[test]
    fn is_multiple_of_exact_row() {
        // Vendor source: num-integer 0.1.46 Integer trait doc-examples.
        // Exact row: 6.is_multiple_of(3) == true.
        assert!((6i32).is_multiple_of(&3));
    }

    #[test]
    fn is_even_false_exact_row() {
        // Vendor source: num-integer 0.1.46 Integer trait doc-examples.
        // Exact row: 7.is_even() == false.
        assert!(!(7i32).is_even());
    }

    #[test]
    fn is_even_true_exact_row() {
        // Vendor source: num-integer 0.1.46 Integer trait doc-examples.
        // Exact row: 8.is_even() == true.
        assert!((8i32).is_even());
    }
}
