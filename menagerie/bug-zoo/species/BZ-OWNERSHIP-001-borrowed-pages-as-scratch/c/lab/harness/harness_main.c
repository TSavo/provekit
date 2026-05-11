/*
 * BZ-OWNERSHIP-001 lab harness (buggy): demonstrates borrow violation.
 *
 * process_borrowed_buf clobbers the caller's buffer while searching for
 * a target byte.  After the call, the original data is destroyed.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Declaration matching the library implementation */
int process_borrowed_buf(char *buf, unsigned int buf_len,
                         unsigned int used, char target);

int main(void)
{
    /* Set up a buffer the "caller" owns */
    char caller_buf[] = "hello, world!";
    unsigned int len = (unsigned int)sizeof(caller_buf) - 1;
    int result;

    printf("before: \"%s\"\n", caller_buf);

    result = process_borrowed_buf(caller_buf, len, len, 'o');

    /* The bug: caller_buf is now zeroed -- borrowed data was clobbered */
    printf("after:  \"%.*s\" (first %u bytes)\n", len, caller_buf, len);
    printf("result: %d (last 'o' at index)\n", result);

    /* Verify the violation occurred: at least one byte is zeroed */
    int clobbered = 0;
    for (unsigned int i = 0; i < len; i++) {
        if (caller_buf[i] == 0) {
            clobbered = 1;
            break;
        }
    }
    if (!clobbered) {
        fprintf(stderr, "FAIL: expected borrow violation did not clobber buf\n");
        return 1;
    }
    printf("ok: borrow violation confirmed -- caller buf clobbered\n");
    return 0;
}
