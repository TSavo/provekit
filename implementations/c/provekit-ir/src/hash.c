/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Native BLAKE3-512 over a UTF-8 JCS string.
 *
 * Links against the BLAKE3 reference C source vendored at
 * tools/blake3-vendored/. The C kit's Makefile compiles those sources
 * with all SIMD paths disabled (-DBLAKE3_NO_AVX2 etc.); no system
 * BLAKE3 install or Python interpreter is required.
 *
 * Output: malloc'd NUL-terminated string of the form
 *   "blake3-512:" + 128 lowercase hex chars
 * Caller is responsible for free().
 */

#include "provekit/ir.h"
#include "blake3.h"
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define PK_BLAKE3_OUT_LEN 64
#define PK_HEX_PREFIX "blake3-512:"
#define PK_HEX_PREFIX_LEN 11
#define PK_HEX_BODY_LEN (PK_BLAKE3_OUT_LEN * 2)
#define PK_HEX_TOTAL_LEN (PK_HEX_PREFIX_LEN + PK_HEX_BODY_LEN)

char *pk_hash_jcs(const char *jcs_string) {
    if (!jcs_string) return NULL;

    blake3_hasher h;
    blake3_hasher_init(&h);
    blake3_hasher_update(&h, jcs_string, strlen(jcs_string));

    uint8_t out[PK_BLAKE3_OUT_LEN];
    blake3_hasher_finalize(&h, out, PK_BLAKE3_OUT_LEN);

    char *result = (char *)malloc(PK_HEX_TOTAL_LEN + 1);
    if (!result) return NULL;
    memcpy(result, PK_HEX_PREFIX, PK_HEX_PREFIX_LEN);

    static const char hex[] = "0123456789abcdef";
    for (size_t i = 0; i < PK_BLAKE3_OUT_LEN; i++) {
        result[PK_HEX_PREFIX_LEN + 2 * i] = hex[(out[i] >> 4) & 0xf];
        result[PK_HEX_PREFIX_LEN + 2 * i + 1] = hex[out[i] & 0xf];
    }
    result[PK_HEX_TOTAL_LEN] = '\0';
    return result;
}
