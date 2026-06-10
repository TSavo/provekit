#[cfg(test)]
mod tests {
    use uuid::Uuid;

    #[test]
    fn test_parse_str_hyphenated_to_string_exact_row() {
        // Vendor source: uuid 1.23.3 src/lib.rs doc-example for to_string.
        // parse_str on simple format, to_string returns hyphenated form.
        let my_uuid = Uuid::parse_str("a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d8").unwrap();
        assert_eq!("a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8", my_uuid.to_string(),);
    }

    #[test]
    fn test_get_version_num_md5_exact_row() {
        // Vendor source: uuid 1.23.3 src/lib.rs doc-example for get_version_num.
        // UUID 02f09a3f-1624-3b1d-8409-44eff7708208 is version 3 (MD5).
        let my_uuid = Uuid::parse_str("02f09a3f-1624-3b1d-8409-44eff7708208").unwrap();
        assert_eq!(3, my_uuid.get_version_num());
    }

    #[test]
    fn test_nil_is_nil_true_exact_row() {
        // Vendor source: uuid 1.23.3 src/lib.rs test_nil.
        // Uuid::nil() reports is_nil() == true.
        let nil = Uuid::nil();
        assert!(nil.is_nil());
    }

    #[test]
    fn test_non_nil_is_nil_false_exact_row() {
        // Vendor source: uuid 1.23.3 src/lib.rs test_nil.
        // A non-nil UUID (v4) reports is_nil() == false.
        let not_nil = Uuid::parse_str("67e55044-10b1-426f-9247-bb680e5fe0c8").unwrap();
        assert!(!not_nil.is_nil());
    }

    #[test]
    fn test_hyphenated_to_string_exact_row() {
        // Vendor source: uuid 1.23.3 src/lib.rs doc-example for hyphenated().
        // as_hyphenated().to_string() returns 36-char hyphen-separated form.
        let uuid = Uuid::parse_str("a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8").unwrap();
        assert_eq!(
            "a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8",
            uuid.hyphenated().to_string(),
        );
    }

    #[test]
    fn test_u128_roundtrip_exact_row() {
        // Vendor source: uuid 1.23.3 src/lib.rs test_u128_roundtrip.
        // from_u128 then as_u128 returns the same value.
        let v_in: u128 = 0xa1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d8;
        let u = Uuid::from_u128(v_in);
        let v_out = u.as_u128();
        assert_eq!(v_in, v_out);
    }

    #[test]
    fn test_nil_to_string_exact_row() {
        // Vendor source: uuid 1.23.3 src/lib.rs some_uuid_nil helper.
        // Uuid::nil() serializes to all-zero hyphenated string.
        let nil = Uuid::nil();
        assert_eq!("00000000-0000-0000-0000-000000000000", nil.to_string(),);
    }

    #[test]
    fn test_parse_str_v4_round_trip_exact_row() {
        // Vendor source: uuid 1.23.3 src/lib.rs some_uuid_v4 helper.
        // parse_str on hyphenated form produces UUID whose to_string is identical.
        let uuid = Uuid::parse_str("67e55044-10b1-426f-9247-bb680e5fe0c8").unwrap();
        assert_eq!("67e55044-10b1-426f-9247-bb680e5fe0c8", uuid.to_string());
    }

    #[test]
    fn test_get_version_num_v4_exact_row() {
        // Vendor source: uuid 1.23.3 src/lib.rs test_get_version: some_uuid_v4 is version 4.
        let uuid = Uuid::parse_str("67e55044-10b1-426f-9247-bb680e5fe0c8").unwrap();
        assert_eq!(4, uuid.get_version_num());
    }
}
