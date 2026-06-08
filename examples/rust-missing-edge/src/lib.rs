// A MISSING-EDGE bug, demonstrated through the real Sugar verbs
// (`sugar mint` -> the rust lifter; `sugar verify` -> compose + discharge).
//
// A bug is a missing edge: a call site whose precondition no producer's
// postcondition establishes, so the composition `post(producer) -> pre(consumer)`
// does not discharge. The unit test passes (it only exercises an input that
// happens to satisfy the precondition), so the bug hides from the test. Only
// lifting the contracts and discharging the seam reveals it.

/// Producer. The lifter derives the postcondition from the body: `result == value`.
/// It says NOTHING about the sign of the result.
pub fn serialize(value: i64) -> i64 {
    value
}

/// Consumer. The leading guard lifts to a precondition: a negative `encoding`
/// panics, so the contract requires `pre = NOT(encoding < 0)` -- the encoding
/// must be non-negative ("canonical").
pub fn content_address(encoding: i64) -> i64 {
    if encoding < 0 {
        panic!("content address requires a non-negative (canonical) encoding");
    }
    encoding
}

/// The seam. `serialize`'s post (`result == value`) does not establish
/// `content_address`'s pre (`encoding >= 0`): for `value < 0` the precondition
/// is violated. The `post -> pre` edge is MISSING; `verify` must refuse it.
pub fn address_of(value: i64) -> i64 {
    content_address(serialize(value))
}

#[cfg(test)]
mod tests {
    use super::address_of;

    /// Passes: this input satisfies the (undischarged) precondition by luck.
    /// The bug -- `address_of(-1)` panics -- is exactly the missing edge that
    /// the test does NOT catch but lift + compose + discharge does.
    #[test]
    fn address_of_nonneg_round_trips() {
        assert_eq!(address_of(7), 7);
    }
}
