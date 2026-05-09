/* BZ-COMPOSITION-001 lab, C side, public declarations. */
#ifndef BZ_COMPOSITION_001_CHAIN_H
#define BZ_COMPOSITION_001_CHAIN_H

#include <stddef.h>
#include <stdint.h>

/* double: pre  none
 *         post returns x * 2 with int32_t wrap on overflow
 */
int32_t bz_double(int32_t x);

/* keep_positive: pre  none
 *                post returns 1 iff x is strictly greater than zero, else 0
 */
int bz_keep_positive(int32_t x);

/* sum: pre  xs is a valid pointer to n int32_t elements (n may be 0)
 *      post returns the wrapping int32_t sum of xs[0..n)
 */
int32_t bz_sum(const int32_t *xs, size_t n);

/* vec_double_then_filter_positive_then_sum:
 *   pre  input is a valid pointer to n int32_t elements (n may be 0)
 *   post returns sum over { bz_double(x) for x in input
 *                           if bz_keep_positive(bz_double(x)) }
 *        with int32_t wrap on overflow
 *   note: caller-provided buf must hold at least n int32_t elements; it is
 *         used as scratch storage only and need not be initialized.
 */
int32_t bz_vec_double_then_filter_positive_then_sum(const int32_t *input,
                                                    size_t n,
                                                    int32_t *buf);

#endif /* BZ_COMPOSITION_001_CHAIN_H */
