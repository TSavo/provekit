#[cfg(test)]
mod tests {
    use url::Url;

    #[test]
    fn test_serialization_addslash_contradiction() {
        // Negative control derived from url 2.5.8 tests/unit.rs::test_serialization,
        // exact row ("http://addslash.com", "http://addslash.com/").
        // The real result is "http://addslash.com/" but we assert both that AND
        // "http://addslash.com" (without trailing slash), which is a contradiction.
        let url = Url::parse("http://addslash.com").unwrap();
        assert_eq!(url.as_str(), "http://addslash.com/");
        assert_eq!(url.as_str(), "http://addslash.com");
    }
}
