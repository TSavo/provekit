// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-rust-std: Rust stdlib @sugar shim.
//
// The Rust kit owns these Rust names and their package/proof resolution. The
// CLI only sees normalized contracts and bridges over RPC.

#![allow(non_snake_case)]

pub const PROVEKIT_PROOF_BYTES: &[u8] = include_bytes!(
    "../blake3-512:51b94a3319761e949da0a63ba04fa77861fda0de6bf145a3169b2690d47569fdd8e8fa8970feee1706530f875cbd1b4627d8d8944a2affbf7c03b5f093a73e82.proof"
);

#[provekit::sugar(
    concept = "library:rust-option-some",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn Some<T>(value: T) -> Option<T> {
    Option::Some(value)
}

#[provekit::sugar(
    concept = "library:rust-result-ok",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn Ok<T, E>(value: T) -> Result<T, E> {
    Result::Ok(value)
}

#[provekit::sugar(
    concept = "library:rust-result-err",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn Err<T, E>(error: E) -> Result<T, E> {
    Result::Err(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_and_result_constructors_match_std() {
        assert_eq!(Some(1), Option::Some(1));
        assert_eq!(Ok::<i32, &str>(2), Result::Ok(2));
        assert_eq!(Err::<i32, &str>("no"), Result::Err("no"));
    }
}
