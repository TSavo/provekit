/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Deterministic CBOR encoder. RFC 8949 §4.2.1 rules:
 *   - shortest-form integer encoding (smallest of short / u8 / u16 / u32 / u64)
 *   - definite-length items only
 *   - map keys sorted in bytewise lex order of their CBOR-encoded form
 *     (the sort happens in the proof-envelope builder; this layer only
 *     emits typed primitives).
 *
 * Mirrors implementations/rust/provekit-proof-envelope/src/cbor.rs and
 * implementations/cpp/provekit/proof-envelope/cbor.cpp 1:1.
 */

#include "provekit/self_contracts.h"

int pksc_cbor_append_head(pksc_bytes *out, pksc_cbor_major major, uint64_t arg) {
    uint8_t mt = (uint8_t)((int)major) << 5;
    uint8_t buf[9];
    size_t  n = 0;

    if (arg < 24) {
        buf[n++] = (uint8_t)(mt | (uint8_t)arg);
    } else if (arg <= 0xFFu) {
        buf[n++] = (uint8_t)(mt | 24);
        buf[n++] = (uint8_t)arg;
    } else if (arg <= 0xFFFFu) {
        buf[n++] = (uint8_t)(mt | 25);
        buf[n++] = (uint8_t)((arg >> 8) & 0xFF);
        buf[n++] = (uint8_t)(arg & 0xFF);
    } else if (arg <= 0xFFFFFFFFu) {
        buf[n++] = (uint8_t)(mt | 26);
        buf[n++] = (uint8_t)((arg >> 24) & 0xFF);
        buf[n++] = (uint8_t)((arg >> 16) & 0xFF);
        buf[n++] = (uint8_t)((arg >> 8) & 0xFF);
        buf[n++] = (uint8_t)(arg & 0xFF);
    } else {
        buf[n++] = (uint8_t)(mt | 27);
        for (int i = 7; i >= 0; --i) {
            buf[n++] = (uint8_t)((arg >> (i * 8)) & 0xFF);
        }
    }
    return pksc_bytes_append(out, buf, n);
}

int pksc_cbor_encode_uint(pksc_bytes *out, uint64_t value) {
    return pksc_cbor_append_head(out, PKSC_CBOR_UINT, value);
}

int pksc_cbor_encode_bstr(pksc_bytes *out, const uint8_t *bytes, size_t n) {
    if (pksc_cbor_append_head(out, PKSC_CBOR_BSTR, (uint64_t)n) != 0) return -1;
    return pksc_bytes_append(out, bytes, n);
}

int pksc_cbor_encode_tstr(pksc_bytes *out, const char *utf8) {
    size_t n = 0;
    if (utf8) {
        const char *p = utf8;
        while (*p) ++p;
        n = (size_t)(p - utf8);
    }
    if (pksc_cbor_append_head(out, PKSC_CBOR_TSTR, (uint64_t)n) != 0) return -1;
    return pksc_bytes_append(out, (const uint8_t *)utf8, n);
}

int pksc_cbor_encode_array_head(pksc_bytes *out, uint64_t count) {
    return pksc_cbor_append_head(out, PKSC_CBOR_ARRAY, count);
}

int pksc_cbor_encode_map_head(pksc_bytes *out, uint64_t count) {
    return pksc_cbor_append_head(out, PKSC_CBOR_MAP, count);
}
