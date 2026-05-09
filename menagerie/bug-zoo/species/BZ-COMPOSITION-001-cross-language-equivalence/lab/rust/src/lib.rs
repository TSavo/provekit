// BZ-COMPOSITION-001 lab, Rust side.
//
// Three pure single-line helpers and a chain wrapper. The chain shape and
// arithmetic are line-for-line equivalent to lab/c/chain.c. The lifter is
// expected to emit one FunctionContractMemento per helper plus one
// ComposedFunctionContract for the wrapper, all with empty effect sets.

/// double: pre  none
///         post returns x * 2 with i32 wrap on overflow
#[inline]
pub fn double(x: i32) -> i32 { x.wrapping_mul(2) }

/// keep_positive: pre  none
///                post returns true iff x is strictly greater than zero
#[inline]
pub fn keep_positive(x: i32) -> bool { x > 0 }

/// sum: pre  none
///      post returns the wrapping i32 sum of the slice elements
#[inline]
pub fn sum(xs: &[i32]) -> i32 {
    let mut acc: i32 = 0;
    let mut i: usize = 0;
    while i < xs.len() {
        acc = acc.wrapping_add(xs[i]);
        i += 1;
    }
    acc
}

/// vec_double_then_filter_positive_then_sum:
///   pre  none
///   post returns sum over { double(x) for x in input if keep_positive(double(x)) }
///        with i32 wrap on overflow
pub fn vec_double_then_filter_positive_then_sum(input: &[i32]) -> i32 {
    let mut buf: Vec<i32> = Vec::with_capacity(input.len());
    let mut i: usize = 0;
    while i < input.len() {
        let d = double(input[i]);
        if keep_positive(d) {
            buf.push(d);
        }
        i += 1;
    }
    sum(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_known_corpus() {
        let v = [-3, -1, 0, 1, 2, 4];
        // doubled:           -6, -2, 0, 2, 4, 8
        // kept positive:     2, 4, 8
        // sum:               14
        assert_eq!(vec_double_then_filter_positive_then_sum(&v), 14);
    }

    #[test]
    fn empty_is_zero() {
        let v: [i32; 0] = [];
        assert_eq!(vec_double_then_filter_positive_then_sum(&v), 0);
    }
}
