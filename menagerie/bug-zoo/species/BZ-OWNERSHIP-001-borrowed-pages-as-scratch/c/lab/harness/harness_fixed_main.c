/*
 * BZ-OWNERSHIP-001 lab harness (fixed): demonstrates borrow contract preserved.
 *
 * process_buf_to_dst writes to a separate dst buffer, leaving src intact.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Declaration matching the fixed implementation */
int process_buf_to_dst(const char *src, char *dst, unsigned int buf_len,
                       unsigned int used, char target);

int main(void)
{
    /* Source buffer the caller owns (borrowed, read-only) */
    const char src_buf[] = "hello, world!";
    unsigned int len = (unsigned int)sizeof(src_buf) - 1;

    /* Separate output buffer the callee may write */
    char dst_buf[sizeof(src_buf)];
    memset(dst_buf, 0xFF, sizeof(dst_buf));

    int result;

    printf("before src: \"%s\"\n", src_buf);

    result = process_buf_to_dst(src_buf, dst_buf, len, len, 'o');

    /* Fixed: src_buf must still contain original data */
    printf("after  src: \"%s\"\n", src_buf);
    printf("result: %d (last 'o' at index)\n", result);

    if (strcmp(src_buf, "hello, world!") != 0) {
        fprintf(stderr, "FAIL: src was modified -- borrow contract violated\n");
        return 1;
    }
    printf("ok: borrow contract preserved -- src unchanged\n");
    return 0;
}
