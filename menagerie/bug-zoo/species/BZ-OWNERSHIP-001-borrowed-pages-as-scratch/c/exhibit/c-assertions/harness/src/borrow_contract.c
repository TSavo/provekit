/*
 * BZ-OWNERSHIP-001: borrowed-pages-as-scratch (lab library)
 *
 * Buggy implementation: process_borrowed_buf takes a caller-owned buffer
 * and uses it as scratch space, clobbering [0, used) with zero bytes.
 * The borrow contract (buf is read-only from [0, used)) is violated.
 *
 * Real-world analog: rxkad_verify_packet_{1,2} in net/rxrpc/rxkad.c
 * (CVE-2026-43500): skcipher_request_set_crypt called with same sg as
 * both src and dst, clobbering caller-owned skb fragments in-place.
 */

#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>

static void BUG(void) { fprintf(stderr, "BUG: assertion failed\n"); abort(); }
#define BUG_ON(cond) do { if (cond) BUG(); } while (0)

/*
 * process_borrowed_buf - scan [0, used) for target; BUG: clobbers buf.
 *
 * @buf      : caller-owned buffer; [0, used) is borrowed (read-only)
 * @buf_len  : total capacity of buf
 * @used     : bytes written by caller into buf
 * @target   : byte value to search for
 *
 * Returns index of last occurrence of target in [0, used), or -1.
 *
 * VIOLATION: zeroes buf[0..used) as scratch, destroying caller's data.
 */
int process_borrowed_buf(char *buf, unsigned int buf_len,
                         unsigned int used, char target)
{
    unsigned int i;
    int found = -1;

    BUG_ON(buf == NULL);
    BUG_ON(buf_len == 0);
    BUG_ON(used > buf_len);

    for (i = 0; i < used; i++) {
        if (buf[i] == target)
            found = (int)i;
        buf[i] = 0;   /* VIOLATION: clobbers caller's borrowed buffer */
    }
    return found;
}
