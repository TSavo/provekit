/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Growable byte buffer. Capacity-doubling growth; init/free are
 * idempotent on zero-initialized memory. Returns -1 on alloc failure
 * so callers can short-circuit. No global state.
 */

#include "provekit/self_contracts.h"
#include <stdlib.h>
#include <string.h>

void pksc_bytes_init(pksc_bytes *b) {
    if (!b) return;
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

void pksc_bytes_free(pksc_bytes *b) {
    if (!b) return;
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static int pksc_bytes_reserve(pksc_bytes *b, size_t need) {
    if (b->cap >= need) return 0;
    size_t new_cap = b->cap ? b->cap : 32;
    while (new_cap < need) {
        size_t doubled = new_cap * 2;
        if (doubled < new_cap) return -1; /* overflow */
        new_cap = doubled;
    }
    uint8_t *p = (uint8_t *)realloc(b->data, new_cap);
    if (!p) return -1;
    b->data = p;
    b->cap = new_cap;
    return 0;
}

int pksc_bytes_append(pksc_bytes *b, const uint8_t *src, size_t n) {
    if (!b) return -1;
    if (n == 0) return 0;
    if (pksc_bytes_reserve(b, b->len + n) != 0) return -1;
    memcpy(b->data + b->len, src, n);
    b->len += n;
    return 0;
}

int pksc_bytes_append_byte(pksc_bytes *b, uint8_t c) {
    return pksc_bytes_append(b, &c, 1);
}
