#[cfg(test)]
mod tests {
    use url::{Host, Url};

    #[test]
    fn test_serialization_addslash_exact_row() {
        // Vendor source: url 2.5.8 tests/unit.rs::test_serialization,
        // exact row ("http://addslash.com", "http://addslash.com/").
        let url = Url::parse("http://addslash.com").unwrap();
        assert_eq!(url.as_str(), "http://addslash.com/");
    }

    #[test]
    fn test_serialization_emptyuser_exact_row() {
        // Vendor source: url 2.5.8 tests/unit.rs::test_serialization,
        // exact row ("http://@emptyuser.com/", "http://emptyuser.com/").
        let url = Url::parse("http://@emptyuser.com/").unwrap();
        assert_eq!(url.as_str(), "http://emptyuser.com/");
    }

    #[test]
    fn test_serialization_emptypass_exact_row() {
        // Vendor source: url 2.5.8 tests/unit.rs::test_serialization,
        // exact row ("http://:@emptypass.com/", "http://emptypass.com/").
        let url = Url::parse("http://:@emptypass.com/").unwrap();
        assert_eq!(url.as_str(), "http://emptypass.com/");
    }

    #[test]
    fn host_serialization_ipv6_no_ipv4_compat_exact_rows() {
        // Vendor source: url 2.5.8 tests/unit.rs::host_serialization.
        // Per WHATWG spec, IPv6 serializer does not use IPv4-like syntax.
        assert_eq!(
            Url::parse("http://[0::2]").unwrap().host_str(),
            Some("[::2]")
        );
        assert_eq!(
            Url::parse("http://[0::ffff:0:2]").unwrap().host_str(),
            Some("[::ffff:0:2]")
        );
    }

    #[test]
    fn test_idna_gosu_ok_exact_row() {
        // Vendor source: url 2.5.8 tests/unit.rs::test_idna.
        // IDNA: Cyrillic domain parses successfully.
        assert!("http://goșu.ro".parse::<Url>().is_ok());
    }

    #[test]
    fn test_idna_snowman_host_exact_row() {
        // Vendor source: url 2.5.8 tests/unit.rs::test_idna.
        // IDNA: snowman encoded as punycode xn--n3h.net.
        assert_eq!(
            Url::parse("http://☃.net/").unwrap().host(),
            Some(Host::Domain("xn--n3h.net"))
        );
    }

    #[test]
    fn test_query_with_fragment_exact_row() {
        // Vendor source: url 2.5.8 tests/unit.rs::test_query.
        // Parse yields query "page=2" when fragment is also present.
        let url = Url::parse("https://example.com/products?page=2#fragment").unwrap();
        assert_eq!(url.query(), Some("page=2"));
    }

    #[test]
    fn test_query_none_exact_row() {
        // Vendor source: url 2.5.8 tests/unit.rs::test_query.
        // URL with no query component returns None.
        let url = Url::parse("https://example.com/products").unwrap();
        assert!(url.query().is_none());
    }

    #[test]
    fn test_fragment_some_exact_row() {
        // Vendor source: url 2.5.8 tests/unit.rs::test_fragment.
        // Fragment component is returned exactly as parsed.
        let url = Url::parse("https://example.com/#fragment").unwrap();
        assert_eq!(url.fragment(), Some("fragment"));
    }

    #[test]
    fn test_fragment_none_exact_row() {
        // Vendor source: url 2.5.8 tests/unit.rs::test_fragment.
        // URL with no fragment returns None.
        let url = Url::parse("https://example.com/").unwrap();
        assert_eq!(url.fragment(), None);
    }
}
