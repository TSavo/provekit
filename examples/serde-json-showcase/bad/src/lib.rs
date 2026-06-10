#[cfg(test)]
mod tests {
    #[test]
    fn test_write_bool_contradiction() {
        // Negative control derived from serde_json 1.0.150
        // tests/test.rs::test_write_bool exact row `(true, "true")`.
        let s = serde_json::to_string(&true).unwrap();

        assert_eq!(s, "true");
        assert_eq!(s, "false");
    }
}
