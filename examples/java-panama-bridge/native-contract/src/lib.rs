//! Minimal Rust contract for the Panama bridge target.
//!
//! This crate exists ONLY to mint the single contract row that the Java
//! Panama bridge binds to:
//!
//!   decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion
//!
//! It carries EXACTLY one test assertion — assert_eq!(3, decoded_len_estimate(4))
//! — so the imported .proof contains only the bridge target row and nothing
//! string-theory-shaped. The verify of the showcase is therefore clean (rc=0,
//! all rows discharged) for the good suite; the bad suite's only refusal is the
//! cross-language contradiction.
//!
//! The result value 3 matches base64 0.22.1's own behaviour: the native shim
//! in `native-shim/` links the REAL base64 crate, and base64::decoded_len_estimate(4)
//! returns 3. The arithmetic body here reproduces that one point so the contract
//! is self-contained and integer-only (no string operations).

/// Estimate the decoded length for a base64-encoded input of `encoded_len` bytes.
/// This is base64 0.22.1's own conservative estimate formula
/// (src/engine/general_purpose/decode.rs: `(encoded_len / 4 + (rem > 0)) * 3`).
/// For a 4-byte encoded group (rem == 0) the decoded estimate is 3.
pub fn decoded_len_estimate(encoded_len: usize) -> usize {
    let rem = encoded_len % 4;
    (encoded_len / 4 + (rem > 0) as usize) * 3
}

#[cfg(test)]
mod tests {
    use super::decoded_len_estimate;

    /// The sworn vendor row: a 4-byte encoded group decodes to 3 bytes.
    /// This is the EXACT assertion base64 0.22.1 makes in its own doctest
    /// (src/decode.rs: assert_eq!(3, decoded_len_estimate(4))).
    #[test]
    fn decodes_four_to_three() {
        assert_eq!(3, decoded_len_estimate(4));
    }
}
