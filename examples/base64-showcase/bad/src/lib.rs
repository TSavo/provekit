#[cfg(test)]
mod tests {
    use base64::encoded_len;

    #[test]
    fn test_encoded_len_unpadded_3_contradiction() {
        // Negative control derived from base64 0.22.1 tests/encode.rs::encoded_len_unpadded.
        // The real value of encoded_len(3, false).unwrap() is 4.
        // Asserting both 4 (correct) and 3 (wrong) over the same term is a contradiction.
        let actual = encoded_len(3, false).unwrap();
        assert_eq!(4, actual);
        assert_eq!(3, actual);
    }
}
