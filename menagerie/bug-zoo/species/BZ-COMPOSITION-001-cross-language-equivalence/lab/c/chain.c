/* BZ-COMPOSITION-001 lab, C side.
 *
 * Three pure helpers and a chain wrapper. Structurally equivalent to
 * lab/rust/src/lib.rs: same algebra, same arithmetic, same per-helper
 * pre/post comments. The lifter is expected to emit one
 * FunctionContractMemento per helper plus one ComposedFunctionContract
 * for the wrapper, all with empty effect sets.
 */

#include "chain.h"

/* double: pre  none
 *         post returns x * 2 with int32_t wrap on overflow
 */
int32_t bz_double(int32_t x) {
    return (int32_t)((uint32_t)x * 2u);
}

/* keep_positive: pre  none
 *                post returns 1 iff x is strictly greater than zero, else 0
 */
int bz_keep_positive(int32_t x) {
    return x > 0 ? 1 : 0;
}

/* sum: pre  xs is a valid pointer to n int32_t elements (n may be 0)
 *      post returns the wrapping int32_t sum of xs[0..n)
 */
int32_t bz_sum(const int32_t *xs, size_t n) {
    int32_t acc = 0;
    size_t i = 0;
    while (i < n) {
        acc = (int32_t)((uint32_t)acc + (uint32_t)xs[i]);
        i += 1;
    }
    return acc;
}

/* vec_double_then_filter_positive_then_sum:
 *   pre  input is a valid pointer to n int32_t elements (n may be 0)
 *   post returns sum over { bz_double(x) for x in input
 *                           if bz_keep_positive(bz_double(x)) }
 *        with int32_t wrap on overflow
 */
int32_t bz_vec_double_then_filter_positive_then_sum(const int32_t *input,
                                                    size_t n,
                                                    int32_t *buf) {
    size_t kept = 0;
    size_t i = 0;
    while (i < n) {
        int32_t d = bz_double(input[i]);
        if (bz_keep_positive(d)) {
            buf[kept] = d;
            kept += 1;
        }
        i += 1;
    }
    return bz_sum(buf, kept);
}
