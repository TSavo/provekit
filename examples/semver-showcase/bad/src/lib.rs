#[cfg(test)]
mod tests {
    use semver::Version;

    #[test]
    fn test_parse_major_field_contradiction() {
        // Negative control derived from semver 1.0.28
        // tests/test_version.rs::test_parse exact row: version("1.2.3").major == 1.
        // The real major is 1 but we also assert it equals 2, which is a contradiction.
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.major, 2);
    }
}
