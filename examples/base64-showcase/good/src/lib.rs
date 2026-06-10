#[cfg(test)]
mod tests {
    use base64::decoded_len_estimate;
    use base64::encoded_len;

    #[test]
    fn test_encoded_len_unpadded_0_exact_row() {
        // Vendor source: base64 0.22.1 tests/encode.rs::encoded_len_unpadded.
        // Exact row: encoded_len(0, false) == Some(0).
        assert_eq!(0, encoded_len(0, false).unwrap());
    }

    #[test]
    fn test_encoded_len_unpadded_1_exact_row() {
        // Vendor source: base64 0.22.1 tests/encode.rs::encoded_len_unpadded.
        // Exact row: encoded_len(1, false) == Some(2).
        assert_eq!(2, encoded_len(1, false).unwrap());
    }

    #[test]
    fn test_encoded_len_unpadded_2_exact_row() {
        // Vendor source: base64 0.22.1 tests/encode.rs::encoded_len_unpadded.
        // Exact row: encoded_len(2, false) == Some(3).
        assert_eq!(3, encoded_len(2, false).unwrap());
    }

    #[test]
    fn test_encoded_len_unpadded_3_exact_row() {
        // Vendor source: base64 0.22.1 tests/encode.rs::encoded_len_unpadded.
        // Exact row: encoded_len(3, false) == Some(4).
        assert_eq!(4, encoded_len(3, false).unwrap());
    }

    #[test]
    fn test_encoded_len_unpadded_5_exact_row() {
        // Vendor source: base64 0.22.1 tests/encode.rs::encoded_len_unpadded.
        // Exact row: encoded_len(5, false) == Some(7).
        assert_eq!(7, encoded_len(5, false).unwrap());
    }

    #[test]
    fn test_encoded_len_padded_1_exact_row() {
        // Vendor source: base64 0.22.1 tests/encode.rs::encoded_len_padded.
        // Exact row: encoded_len(1, true) == Some(4).
        assert_eq!(4, encoded_len(1, true).unwrap());
    }

    #[test]
    fn test_encoded_len_padded_4_exact_row() {
        // Vendor source: base64 0.22.1 tests/encode.rs::encoded_len_padded.
        // Exact row: encoded_len(4, true) == Some(8).
        assert_eq!(8, encoded_len(4, true).unwrap());
    }

    #[test]
    fn test_encoded_len_padded_7_exact_row() {
        // Vendor source: base64 0.22.1 tests/encode.rs::encoded_len_padded.
        // Exact row: encoded_len(7, true) == Some(12).
        assert_eq!(12, encoded_len(7, true).unwrap());
    }

    #[test]
    fn test_decoded_len_estimate_4_exact_row() {
        // Vendor source: base64 0.22.1 src/decode.rs::decoded_len_est.
        // Exact row: decoded_len_estimate(4) == 3.
        assert_eq!(3, decoded_len_estimate(4));
    }
}
