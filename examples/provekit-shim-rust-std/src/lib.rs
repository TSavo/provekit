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

    fn plus_one(value: i32) -> i32 {
        value + 1
    }

    fn some_plus_one(value: i32) -> Option<i32> {
        Option::Some(value + 1)
    }

    fn tag_error(_: &'static str) -> &'static str {
        "tagged"
    }

    #[test]
    fn option_and_result_constructors_match_std() {
        assert_eq!(Some(1), Option::Some(1));
        assert_eq!(Ok::<i32, &str>(2), Result::Ok(2));
        assert_eq!(Err::<i32, &str>("no"), Result::Err("no"));
    }

    #[test]
    fn option_constructor_tag_and_projection_algebra() {
        assert!(Some(1).is_some());
        assert!(!Some(1).is_none());
        assert_ne!(Some(1), Option::None);
        assert_eq!(Some(1).unwrap(), 1);
        assert_eq!(Some(1).unwrap_or(9), 1);
        assert_eq!(Option::<i32>::None.unwrap_or(9), 9);
    }

    #[test]
    fn option_functor_and_result_conversion_algebra() {
        assert_eq!(Some(1).map(plus_one), Option::Some(2));
        assert_eq!(Some(1).and_then(some_plus_one), Option::Some(2));
        assert_eq!(Some(1).ok_or("missing"), Result::Ok(1));
        assert_eq!(Option::<i32>::None.ok_or("missing"), Result::Err("missing"));
    }

    #[test]
    fn result_constructor_tag_and_projection_algebra() {
        assert!(Ok::<i32, &str>(2).is_ok());
        assert!(!Ok::<i32, &str>(2).is_err());
        assert!(Err::<i32, &str>("no").is_err());
        assert!(!Err::<i32, &str>("no").is_ok());
        assert_eq!(Ok::<i32, &str>(2).unwrap(), 2);
        assert_eq!(Err::<i32, &str>("no").unwrap_err(), "no");
        assert_eq!(Ok::<i32, &str>(2).unwrap_or(9), 2);
        assert_eq!(Err::<i32, &str>("no").unwrap_or(9), 9);
    }

    #[test]
    fn result_functor_and_option_conversion_algebra() {
        assert_eq!(Ok::<i32, &str>(2).map(plus_one), Result::Ok(3));
        assert_eq!(Err::<i32, &str>("no").map(plus_one), Result::Err("no"));
        assert_eq!(Ok::<i32, &str>(2).map_err(tag_error), Result::Ok(2));
        assert_eq!(
            Err::<i32, &str>("no").map_err(tag_error),
            Result::Err("tagged")
        );
        assert_eq!(Ok::<i32, &str>(2).ok(), Option::Some(2));
        assert_eq!(Err::<i32, &str>("no").ok(), Option::None);
        assert_eq!(Ok::<i32, &str>(2).err(), Option::None);
        assert_eq!(Err::<i32, &str>("no").err(), Option::Some("no"));
    }
}
