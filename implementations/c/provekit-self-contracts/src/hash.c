/* SPDX-License-Identifier: Apache-2.0 */
/*
 * BLAKE3-512 over a raw byte buffer, returning a malloc'd
 * "blake3-512:<128 lowercase hex chars>" string.
 *
 * Companion to provekit-ir/src/hash.c (which takes a NUL-terminated JCS
 * UTF-8 string). The proof envelope hashes raw CBOR bytes, which can
 * contain any value 0x00..0xFF -- so it needs the explicit-length form.
 *
 * Links against the same BLAKE3 reference source vendored at
 * tools/blake3-vendored/, compiled with all SIMD paths disabled.
 */

#include "provekit/self_contracts.h"
#include "blake3.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define PKSC_BLAKE3_OUT_LEN 64
#define PKSC_HEX_PREFIX "blake3-512:"
#define PKSC_HEX_PREFIX_LEN 11
#define PKSC_HEX_BODY_LEN (PKSC_BLAKE3_OUT_LEN * 2)
#define PKSC_HEX_TOTAL_LEN (PKSC_HEX_PREFIX_LEN + PKSC_HEX_BODY_LEN)

char *pksc_blake3_512_cid(const uint8_t *data, size_t len) {
    blake3_hasher h;
    blake3_hasher_init(&h);
    if (len > 0 && data) blake3_hasher_update(&h, data, len);

    uint8_t out[PKSC_BLAKE3_OUT_LEN];
    blake3_hasher_finalize(&h, out, PKSC_BLAKE3_OUT_LEN);

    char *result = (char *)malloc(PKSC_HEX_TOTAL_LEN + 1);
    if (!result) return NULL;
    memcpy(result, PKSC_HEX_PREFIX, PKSC_HEX_PREFIX_LEN);

    static const char hex[] = "0123456789abcdef";
    for (size_t i = 0; i < PKSC_BLAKE3_OUT_LEN; i++) {
        result[PKSC_HEX_PREFIX_LEN + 2*i]     = hex[(out[i] >> 4) & 0xF];
        result[PKSC_HEX_PREFIX_LEN + 2*i + 1] = hex[ out[i]       & 0xF];
    }
    result[PKSC_HEX_TOTAL_LEN] = '\0';
    return result;
}
