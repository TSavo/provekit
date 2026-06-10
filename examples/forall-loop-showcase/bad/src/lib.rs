#[cfg(test)]
mod tests {
    fn g(_x: i64) -> i64 {
        1
    }

    // The loop lifts to forall x. (0 <= x < 3 => g(x) == 1). The extra
    // assertion g(2) == 2 contradicts it at the in-range point x = 2: the
    // verifier instantiates the universal at 2 (g(2) == 1) against g(2) == 2 and
    // refuses the conjunction. cargo test also fails (g(2) is 1), so the witness
    // refuses too.
    #[test]
    fn g_range_contradiction() {
        for x in 0..3 {
            assert_eq!(g(x), 1);
        }
        assert_eq!(g(2), 2);
    }
}
