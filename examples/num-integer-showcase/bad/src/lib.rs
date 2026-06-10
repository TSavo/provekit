#[cfg(test)]
mod tests {
    #[test]
    fn gcd_contradiction() {
        // Negative control derived from num-integer 0.1.46 doc-examples.
        // The real result of gcd(12, 8) is 4, but we also assert it equals 6,
        // which is a contradiction. z3 must see this as UNSAT.
        assert_eq!(num_integer::gcd(12i32, 8i32), 4i32);
        assert_eq!(num_integer::gcd(12i32, 8i32), 6i32);
    }
}
