/*
 * BZ-OWNERSHIP-001: borrowed-pages-as-scratch (fixed implementation)
 *
 * Fixed: takes separate src (borrowed, read-only) and dst (writable,
 * caller-provided).  The BUG_ON(dst == src) assertion explicitly captures
 * the non-aliasing contract: caller must not pass the same buffer for both
 * src and dst.
 *
 * The precondition neq(dst, src) appears in the lifted ProofIR, making the
 * contract machine-checkable.
 */

#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>

static void BUG(void) { fprintf(stderr, "BUG: assertion failed\n"); abort(); }
#define BUG_ON(cond) do { if (cond) BUG(); } while (0)

/*
 * process_buf_to_dst - scan [0, used) for target; write zeroed scratch to dst.
 *
 * @src      : caller-owned source buffer; [0, used) is borrowed (read-only)
 * @dst      : caller-provided output buffer (writable, must not alias src)
 * @buf_len  : capacity of both src and dst
 * @used     : bytes written by caller into src
 * @target   : byte value to search for
 *
 * Returns index of last occurrence of target in [0, used), or -1.
 *
 * FIX: reads from src only; writes only to dst.  Non-aliasing enforced by
 * the BUG_ON(dst == src) contract assertion.
 */
int process_buf_to_dst(const char *src, char *dst, unsigned int buf_len,
                       unsigned int used, char target)
{
    unsigned int i;
    int found = -1;

    BUG_ON(src == NULL);
    BUG_ON(dst == NULL);
    BUG_ON(dst == src);
    BUG_ON(buf_len == 0);
    BUG_ON(used > buf_len);

    for (i = 0; i < used; i++) {
        if (src[i] == target)
            found = (int)i;
        dst[i] = 0;   /* writes only to dst, leaving src intact */
    }
    return found;
}
