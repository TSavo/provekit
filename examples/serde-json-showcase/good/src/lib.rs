#[cfg(test)]
mod tests {
    #[test]
    fn test_write_null_exact_row() {
        // Vendor source: serde_json 1.0.150 tests/test.rs::test_write_null,
        // via test_encode_ok: `let s = to_string(value).unwrap(); assert_eq!(s, out);`
        let s = serde_json::to_string(&()).unwrap();

        assert_eq!(s, "null");
    }

    #[test]
    fn test_write_u64_exact_row() {
        // Vendor source: serde_json 1.0.150 tests/test.rs::test_write_u64,
        // exact row `(3u64, "3")` through test_encode_ok.
        let s = serde_json::to_string(&3u64).unwrap();

        assert_eq!(s, "3");
    }

    #[test]
    fn test_write_str_exact_row() {
        // Vendor source: serde_json 1.0.150 tests/test.rs::test_write_str,
        // exact row `("foo", "\"foo\"")` through test_encode_ok.
        let s = serde_json::to_string(&"foo").unwrap();

        assert_eq!(s, "\"foo\"");
    }

    #[test]
    fn test_write_bool_exact_row() {
        // Vendor source: serde_json 1.0.150 tests/test.rs::test_write_bool,
        // exact row `(true, "true")` through test_encode_ok.
        let s = serde_json::to_string(&true).unwrap();

        assert_eq!(s, "true");
    }
}
