#[cfg(test)]
mod tests {
    use regex::Regex;

    #[test]
    fn invalid_regexes_no_crash_exact_rows() {
        // Vendor source: regex 1.12.4 tests/regression.rs::invalid_regexes_no_crash.
        assert!(Regex::new("(*)").is_err());
        assert!(Regex::new("(?:?)").is_err());
        assert!(Regex::new("(?)").is_err());
        assert!(Regex::new("*").is_err());
    }

    #[test]
    fn invalid_repetition_exact_row() {
        // Vendor source: regex 1.12.4
        // tests/regression.rs::regression_invalid_repetition_expr.
        assert!(Regex::new("(?m){1,1}").is_err());
    }

    #[test]
    fn valid_flags_expression_exact_row() {
        // Vendor source: regex 1.12.4
        // tests/regression.rs::regression_invalid_flags_expression.
        assert!(Regex::new("(((?x)))").is_ok());
    }

    #[test]
    fn fail_branch_prevents_match_exact_row() {
        // Vendor source: regex 1.12.4
        // tests/regression_fuzz.rs::fail_branch_prevents_match.
        assert!(Regex::new(r".*[a&&b]A|B").unwrap().is_match("B"));
    }
}
