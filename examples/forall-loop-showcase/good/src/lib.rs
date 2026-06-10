#[cfg(test)]
mod tests {
    // A pure function over the loop range. The bounded loop below lifts to the
    // guarded universal: forall x. (0 <= x < 3 => g(x) == 1).
    fn g(_x: i64) -> i64 {
        1
    }

    #[test]
    fn g_is_one_on_range() {
        for x in 0..3 {
            assert_eq!(g(x), 1);
        }
    }
}
