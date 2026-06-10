#[cfg(test)]
mod tests {
    use uuid::Uuid;

    #[test]
    fn test_parse_str_hyphenated_to_string_contradiction() {
        // Negative control derived from uuid 1.23.3 src/lib.rs doc-example for to_string.
        // The real result is "a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8" but we also assert
        // "a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d8" (simple, no hyphens), which is a contradiction.
        let my_uuid = Uuid::parse_str("a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d8").unwrap();
        assert_eq!("a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8", my_uuid.to_string(),);
        assert_eq!("a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d8", my_uuid.to_string(),);
    }
}
