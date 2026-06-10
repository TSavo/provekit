#[cfg(test)]
mod tests {
    use regex::Regex;

    #[test]
    fn valid_flags_expression_contradiction() {
        // Negative control derived from regex 1.12.4
        // tests/regression.rs::regression_invalid_flags_expression.
        assert!(Regex::new("(((?x)))").is_ok());
        assert!(!Regex::new("(((?x)))").is_ok());
    }
}
