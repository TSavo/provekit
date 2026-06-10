#[cfg(test)]
mod tests {
    use semver::{Version, VersionReq};

    #[test]
    fn test_parse_major_field_exact_row() {
        // Vendor source: semver 1.0.28 tests/test_version.rs::test_parse,
        // exact row: version("1.2.3") => Version::new(1, 2, 3), major == 1.
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
    }

    #[test]
    fn test_parse_minor_field_exact_row() {
        // Vendor source: semver 1.0.28 tests/test_version.rs::test_parse,
        // exact row: version("1.2.3") => Version::new(1, 2, 3), minor == 2.
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.minor, 2);
    }

    #[test]
    fn test_parse_patch_field_exact_row() {
        // Vendor source: semver 1.0.28 tests/test_version.rs::test_parse,
        // exact row: version("1.2.3") => Version::new(1, 2, 3), patch == 3.
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_parse_to_string_exact_row() {
        // Vendor source: semver 1.0.28 tests/test_version.rs::test_display,
        // exact row: version("1.2.3").to_string() == "1.2.3".
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.to_string(), "1.2.3");
    }

    #[test]
    fn test_version_req_exact_matches_true_exact_row() {
        // Vendor source: semver 1.0.28 tests/test_version_req.rs::test_exact,
        // req("=1.0.0") matches "1.0.0" => true.
        let r = VersionReq::parse("=1.0.0").unwrap();
        let v = Version::parse("1.0.0").unwrap();
        assert!(r.matches(&v));
    }

    #[test]
    fn test_version_req_exact_matches_false_exact_row() {
        // Vendor source: semver 1.0.28 tests/test_version_req.rs::test_exact,
        // req("=1.0.0") does not match "1.0.1" => false.
        let r = VersionReq::parse("=1.0.0").unwrap();
        let v = Version::parse("1.0.1").unwrap();
        assert!(!r.matches(&v));
    }

    #[test]
    fn test_version_ordering_lt_exact_row() {
        // Vendor source: semver 1.0.28 tests/test_version.rs::test_lt,
        // version("0.0.0") < version("0.0.1") == true.
        let a = Version::parse("0.0.0").unwrap();
        let b = Version::parse("0.0.1").unwrap();
        assert!(a < b);
    }

    #[test]
    fn test_version_req_caret_matches_true_exact_row() {
        // Vendor source: semver 1.0.28 tests/test_version_req.rs::test_basic,
        // req("1.0.0") (caret) matches "1.1.0" => true.
        let r = VersionReq::parse("1.0.0").unwrap();
        let v = Version::parse("1.1.0").unwrap();
        assert!(r.matches(&v));
    }
}
