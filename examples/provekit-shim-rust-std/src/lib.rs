// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-rust-std: Rust stdlib sugar shim.
//
// The Rust kit owns these Rust names and their package/proof resolution. The
// CLI only sees normalized contracts and bridges over RPC.
//
// Design notes:
//   Every wrapper below is published by the rust-bind (@sugar) surface as a
//   NAMED contract: kind="contract" carrying a trivial return-shape post
//   (out = function_name(<params>)) and NO pre. This flips each call site from
//   an untracked lift-gap (which vacuous-passes) to a substrate-named,
//   bridgeable contract. That naming is the value this shim delivers.
//
//   EMPIRICAL NOTE ON PRECONDITIONS (verified by minting this shim and
//   inspecting the resulting .proof, not by reading the lifter in isolation):
//   the @sugar / library-bindings lift path in provekit-walk does NOT lift a
//   leading assert! into a contract pre. The binding-entry + contract emitter
//   in provekit-walk/src/bin/walk_rpc.rs builds the contract post-only and is
//   explicit about why: emitting kind="function-contract" would trigger
//   body-discharge, which substrate-honestly refuses a contract that has
//   formals but no real body-derived precondition; the sugar surface has no
//   such precondition without a deeper lifter, so it stays out of
//   body-discharge by design. A leading assert! survives only as body_text /
//   ast_template source. (lift_function_precondition itself works and is used
//   by other walk surfaces: shadow, dropper, the walk.lift_pre RPC, but NOT by
//   the surface this shim mints through. The minted proof contains zero is_ok
//   / is_err predicate names and no "pre" field, confirming this.)
//
//   Consequence: the wrappers marked PARTIAL keep a genuine RUNTIME
//   precondition in their Rust body (the assert! truly panics on violation,
//   which is honest), but their PUBLISHED contract is post-only, identical in
//   shape to the TOTAL wrappers. We do NOT claim the substrate discharges the
//   precondition. Recovering a real dischargeable pre for these would require
//   routing sugar bodies through lift_function_precondition (a change to the
//   walk lifter or a new lift surface), which is outside this shim's scope.
//
//   SKIPPED higher-order callees (map closures, filter, collect, ok_or_else,
//   etc.): the lift layer has no closure/callback term representation; modeling
//   a single monomorphic instance would misrepresent the general contract.
//     Named and justified in the return note.

#![allow(non_snake_case)]

pub const PROVEKIT_PROOF_BYTES: &[u8] = include_bytes!(
    "../blake3-512:1fea8ecbb7b618f8578d49c969bdd65d86fc050d50926f5a8cee1cabf5214fbb643829518dbbc72636c7605542bf79b71415a9491b65e490c4a62c2fafbd4451.proof"
);

// ---------------------------------------------------------------------------
// Existing constructors (total, no precondition)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Task-99 canonical set (required)
// ---------------------------------------------------------------------------

// TOTAL: Display::to_string: never panics, total for all T: ToString.
#[provekit::sugar(
    concept = "library:rust-to-string",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn to_string<T: ToString>(value: &T) -> String {
    value.to_string()
}

// TOTAL: slice/Vec::len: pure observer, always returns usize.
#[provekit::sugar(
    concept = "library:rust-len",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn len(slice: &[u8]) -> usize {
    slice.len()
}

// TOTAL: Clone::clone: total for all T: Clone.
#[provekit::sugar(
    concept = "library:rust-clone",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn clone<T: Clone>(value: &T) -> T {
    value.clone()
}

// TOTAL: From/Into conversion: total for all T: From<U>.
#[provekit::sugar(
    concept = "library:rust-from",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn from<T, U: Into<T>>(value: U) -> T {
    value.into()
}

// TOTAL: Default::default: total for all T: Default.
#[provekit::sugar(
    concept = "library:rust-default",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn default<T: Default>() -> T {
    T::default()
}

// PARTIAL: Option::unwrap. Runtime precondition: opt.is_some(). The published
// contract is post-only; the assert! is not lifted as a pre (see header note).
#[provekit::sugar(
    concept = "library:rust-option-unwrap",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn option_unwrap<T>(opt: Option<T>) -> T {
    assert!(opt.is_some());
    opt.unwrap()
}

// PARTIAL: Result::unwrap. Runtime precondition: result.is_ok(). Published
// contract is post-only; the assert! is not lifted as a pre (see header note).
#[provekit::sugar(
    concept = "library:rust-result-unwrap",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn result_unwrap<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    assert!(result.is_ok());
    result.unwrap()
}

// PARTIAL: Result::expect. Runtime precondition: result.is_ok() (same as
// result_unwrap); separate concept for a distinctly named contract. Published
// contract is post-only; the assert! is not lifted as a pre (see header note).
#[provekit::sugar(
    concept = "library:rust-result-expect",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn result_expect<T, E: std::fmt::Debug>(result: Result<T, E>, msg: &str) -> T {
    assert!(result.is_ok());
    result.expect(msg)
}

// ---------------------------------------------------------------------------
// Measured-frequency extensions
// ---------------------------------------------------------------------------

// TOTAL: Vec::new / HashMap::new cannot be modeled generically here (no
// universal "new" concept). Model the Vec case, which is the dominant usage.
#[provekit::sugar(
    concept = "library:rust-vec-new",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn vec_new<T>() -> Vec<T> {
    Vec::new()
}

// TOTAL: String::new: frequent in CLI code.
#[provekit::sugar(
    concept = "library:rust-string-new",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn string_new() -> String {
    String::new()
}

// TOTAL: slice::get. This is the checked accessor: it returns None out of
// bounds rather than panicking, so it has NO precondition. The bounds
// obligation lives in the returned Option, not in a contract pre. (An earlier
// draft asserted index < slice.len() here, which both fabricated a precondition
// the real function does not have and would not lift anyway; removed.)
#[provekit::sugar(
    concept = "library:rust-slice-get",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn slice_get(slice: &[u8], index: usize) -> Option<&u8> {
    slice.get(index)
}

// TOTAL: Option::is_some: pure predicate observer.
#[provekit::sugar(
    concept = "library:rust-option-is-some",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn is_some<T>(opt: &Option<T>) -> bool {
    opt.is_some()
}

// TOTAL: Option::is_none: pure predicate observer.
#[provekit::sugar(
    concept = "library:rust-option-is-none",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn is_none<T>(opt: &Option<T>) -> bool {
    opt.is_none()
}

// TOTAL: str::is_empty / slice::is_empty.
#[provekit::sugar(
    concept = "library:rust-is-empty",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn is_empty(s: &str) -> bool {
    s.is_empty()
}

// TOTAL: str::trim: total, no panic.
#[provekit::sugar(
    concept = "library:rust-str-trim",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn str_trim(s: &str) -> &str {
    s.trim()
}

// TOTAL: str::starts_with.
#[provekit::sugar(
    concept = "library:rust-str-starts-with",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn str_starts_with<'a>(s: &'a str, prefix: &str) -> bool {
    s.starts_with(prefix)
}

// TOTAL: Vec::push: total for all T, mutates vec.
#[provekit::sugar(
    concept = "library:rust-vec-push",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn vec_push<T>(vec: &mut Vec<T>, value: T) {
    vec.push(value)
}

// TOTAL: Option::unwrap_or: total, provides fallback.
#[provekit::sugar(
    concept = "library:rust-option-unwrap-or",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn option_unwrap_or<T>(opt: Option<T>, default: T) -> T {
    opt.unwrap_or(default)
}

// TOTAL: Result::unwrap_or: total, provides fallback.
#[provekit::sugar(
    concept = "library:rust-result-unwrap-or",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn result_unwrap_or<T, E>(result: Result<T, E>, default: T) -> T {
    result.unwrap_or(default)
}

// TOTAL: Option::take: extracts value and leaves None in place.
#[provekit::sugar(
    concept = "library:rust-option-take",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn option_take<T>(opt: &mut Option<T>) -> Option<T> {
    opt.take()
}

// TOTAL: str::join on a slice of strings.
#[provekit::sugar(
    concept = "library:rust-str-join",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn str_join(parts: &[&str], sep: &str) -> String {
    parts.join(sep)
}

// TOTAL: AsRef coercion: models the common .as_ref() identity idiom.
#[provekit::sugar(
    concept = "library:rust-as-ref",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn as_ref<T: AsRef<U>, U: ?Sized>(value: &T) -> &U {
    value.as_ref()
}

// PARTIAL: Result::unwrap_err. Runtime precondition: result.is_err(). Published
// contract is post-only; the assert! is not lifted as a pre (see header note).
#[provekit::sugar(
    concept = "library:rust-result-unwrap-err",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn result_unwrap_err<T: std::fmt::Debug, E>(result: Result<T, E>) -> E {
    assert!(result.is_err());
    result.unwrap_err()
}

// PARTIAL: Option::expect. Runtime precondition: opt.is_some() (same as
// option_unwrap); separate concept for a distinctly named contract. Published
// contract is post-only; the assert! is not lifted as a pre (see header note).
#[provekit::sugar(
    concept = "library:rust-option-expect",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn option_expect<T>(opt: Option<T>, msg: &str) -> T {
    assert!(opt.is_some());
    opt.expect(msg)
}

// TOTAL: String::as_str (high frequency in the CLI: 125 call sites). A String
// always borrows as &str, so there is no precondition.
#[provekit::sugar(
    concept = "library:rust-string-as-str",
    library = "std",
    version = "rust-1",
    family = "concept:family:rust-std",
    loss = [],
)]
pub fn as_str(value: &String) -> &str {
    value.as_str()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    // --- task-99 required set ---

    #[test]
    fn to_string_total_algebra() {
        assert_eq!(to_string(&42i32), "42");
        assert_eq!(to_string(&"hello"), "hello");
        assert_eq!(to_string(&true), "true");
    }

    #[test]
    fn len_total_observer_algebra() {
        let v: Vec<u8> = vec![1, 2, 3];
        assert_eq!(len(&v), 3);
        assert_eq!(len(&[]), 0);
        assert_eq!(len(&[0u8; 5]), 5);
    }

    #[test]
    fn clone_total_identity_algebra() {
        let x = 42i32;
        assert_eq!(clone(&x), x);
        let s = String::from("abc");
        assert_eq!(clone(&s), s);
    }

    #[test]
    fn from_total_conversion_algebra() {
        let n: i64 = from(42i32);
        assert_eq!(n, 42i64);
        let s: String = from("hello");
        assert_eq!(s, "hello");
    }

    #[test]
    fn default_total_produces_zero_values() {
        let n: i32 = default();
        assert_eq!(n, 0);
        let s: String = default();
        assert_eq!(s, "");
        let v: Vec<i32> = default();
        assert!(v.is_empty());
    }

    #[test]
    fn option_unwrap_partial_satisfied_precondition() {
        // satisfied precondition: is_some() holds.
        assert_eq!(option_unwrap(Option::Some(7)), 7);
        assert_eq!(option_unwrap(Option::Some("x")), "x");
    }

    #[test]
    fn result_unwrap_partial_satisfied_precondition() {
        assert_eq!(result_unwrap(Result::Ok::<i32, &str>(5)), 5);
        assert_eq!(result_unwrap(Result::Ok::<&str, &str>("y")), "y");
    }

    #[test]
    fn result_expect_partial_satisfied_precondition() {
        assert_eq!(
            result_expect(Result::Ok::<i32, &str>(5), "must be ok"),
            5
        );
        assert_eq!(
            result_expect(Result::Ok::<&str, &str>("y"), "must be ok"),
            "y"
        );
    }

    // --- measured-frequency extensions ---

    #[test]
    fn vec_new_total_empty() {
        let v: Vec<i32> = vec_new();
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn string_new_total_empty() {
        let s = string_new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn slice_get_total_in_bounds() {
        let data = [10u8, 20, 30];
        assert_eq!(slice_get(&data, 0), Some(&10u8));
        assert_eq!(slice_get(&data, 2), Some(&30u8));
    }

    #[test]
    fn slice_get_total_out_of_bounds_is_none() {
        // get is total: out of bounds is None through our own wrapper, no panic.
        let data = [10u8, 20, 30];
        assert_eq!(slice_get(&data, 5), None);
        assert_eq!(slice_get(&[], 0), None);
    }

    #[test]
    fn is_some_and_is_none_observer_algebra() {
        assert!(is_some(&Option::Some(1)));
        assert!(!is_some(&Option::<i32>::None));
        assert!(is_none(&Option::<i32>::None));
        assert!(!is_none(&Option::Some(1)));
    }

    #[test]
    fn is_empty_observer_algebra() {
        assert!(is_empty(""));
        assert!(!is_empty("x"));
    }

    #[test]
    fn str_trim_total_algebra() {
        assert_eq!(str_trim("  hello  "), "hello");
        assert_eq!(str_trim("no-spaces"), "no-spaces");
        assert_eq!(str_trim(""), "");
    }

    #[test]
    fn str_starts_with_algebra() {
        assert!(str_starts_with("provekit-cli", "provekit"));
        assert!(!str_starts_with("provekit-cli", "cli"));
        assert!(str_starts_with("", ""));
    }

    #[test]
    fn vec_push_total_mutation_algebra() {
        let mut v: Vec<i32> = vec_new();
        vec_push(&mut v, 1);
        vec_push(&mut v, 2);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], 1);
        assert_eq!(v[1], 2);
    }

    #[test]
    fn option_unwrap_or_total_algebra() {
        assert_eq!(option_unwrap_or(Option::Some(3), 9), 3);
        assert_eq!(option_unwrap_or(Option::None, 9), 9);
    }

    #[test]
    fn result_unwrap_or_total_algebra() {
        assert_eq!(result_unwrap_or(Result::Ok::<i32, &str>(3), 9), 3);
        assert_eq!(result_unwrap_or(Result::Err::<i32, &str>("e"), 9), 9);
    }

    #[test]
    fn option_take_algebra() {
        let mut opt = Option::Some(42);
        let taken = option_take(&mut opt);
        assert_eq!(taken, Option::Some(42));
        assert!(opt.is_none());
    }

    #[test]
    fn str_join_total_algebra() {
        assert_eq!(str_join(&["a", "b", "c"], "-"), "a-b-c");
        assert_eq!(str_join(&[], "-"), "");
        assert_eq!(str_join(&["only"], "/"), "only");
    }

    #[test]
    fn as_ref_coercion_algebra() {
        let s = String::from("hello");
        let r: &str = as_ref(&s);
        assert_eq!(r, "hello");
    }

    #[test]
    fn result_unwrap_err_partial_satisfied_precondition() {
        let e: &str = result_unwrap_err(Result::Err::<i32, &str>("boom"));
        assert_eq!(e, "boom");
    }

    #[test]
    fn option_expect_partial_satisfied_precondition() {
        assert_eq!(option_expect(Option::Some(99), "must be some"), 99);
    }

    #[test]
    fn as_str_total_borrow_algebra() {
        let owned = String::from("provekit");
        // as_str borrows the same bytes the String owns.
        assert_eq!(as_str(&owned), "provekit");
        assert_eq!(as_str(&String::new()), "");
    }
}
