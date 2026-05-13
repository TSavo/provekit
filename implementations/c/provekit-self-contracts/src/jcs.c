/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Generic JSON Value tree + JCS canonicalizer (RFC 8785).
 *
 * Rules:
 *   - Object keys sorted by Unicode code-point order. For ASCII-only
 *     keys (which is everything the protocol uses) this collapses to
 *     bytewise strcmp.
 *   - Strings: UTF-8 verbatim (>= U+0080 emitted as raw multi-byte
 *     sequences). Escape `"` and `\` and U+0000..U+001F as `\u00XX`
 *     with lowercase hex.
 *   - Integers: plain decimal (i64).
 *   - true / false / null verbatim.
 *   - No whitespace anywhere.
 *
 * Mirrors implementations/rust/provekit-canonicalizer/src/{value,jcs}.rs
 * and the IR-coupled provekit-ir/src/jcs.c (this file is the generic
 * value-tree variant; the IR variant in provekit-ir handles IR nodes
 * directly without an intermediate Value tree).
 */

#include "provekit/self_contracts.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* --- Value constructors ------------------------------------------------- */

static pksc_value *alloc_value(pksc_value_kind k) {
    pksc_value *v = (pksc_value *)calloc(1, sizeof(*v));
    if (v) v->kind = k;
    return v;
}

static char *dup_str(const char *s) {
    if (!s) return NULL;
    size_t n = strlen(s);
    char *p = (char *)malloc(n + 1);
    if (!p) return NULL;
    memcpy(p, s, n + 1);
    return p;
}

pksc_value *pksc_v_null(void) { return alloc_value(PKSC_V_NULL); }

pksc_value *pksc_v_bool(int b) {
    pksc_value *v = alloc_value(PKSC_V_BOOL);
    if (v) v->as.b = b ? 1 : 0;
    return v;
}

pksc_value *pksc_v_int(int64_t i) {
    pksc_value *v = alloc_value(PKSC_V_INT);
    if (v) v->as.i = i;
    return v;
}

pksc_value *pksc_v_str(const char *s) {
    pksc_value *v = alloc_value(PKSC_V_STR);
    if (!v) return NULL;
    v->as.s = dup_str(s ? s : "");
    if (!v->as.s) { free(v); return NULL; }
    return v;
}

pksc_value *pksc_v_arr_new(void) {
    pksc_value *v = alloc_value(PKSC_V_ARR);
    return v;
}

pksc_value *pksc_v_obj_new(void) {
    pksc_value *v = alloc_value(PKSC_V_OBJ);
    return v;
}

int pksc_v_arr_push(pksc_value *arr, pksc_value *item) {
    if (!arr || arr->kind != PKSC_V_ARR || !item) return -1;
    size_t n = arr->as.arr.n;
    pksc_value **p = (pksc_value **)realloc(arr->as.arr.items, (n + 1) * sizeof(*p));
    if (!p) return -1;
    arr->as.arr.items = p;
    p[n] = item;
    arr->as.arr.n = n + 1;
    return 0;
}

int pksc_v_obj_set(pksc_value *obj, const char *key, pksc_value *value) {
    if (!obj || obj->kind != PKSC_V_OBJ || !key || !value) return -1;
    /* No dedupe — caller is responsible. JCS sorts at emit anyway, and
     * duplicate keys would render the document invalid JSON regardless. */
    if (obj->as.obj.n == obj->as.obj.cap) {
        size_t new_cap = obj->as.obj.cap ? obj->as.obj.cap * 2 : 4;
        pksc_kv *p = (pksc_kv *)realloc(obj->as.obj.entries, new_cap * sizeof(*p));
        if (!p) return -1;
        obj->as.obj.entries = p;
        obj->as.obj.cap = new_cap;
    }
    pksc_kv *e = &obj->as.obj.entries[obj->as.obj.n];
    e->key = dup_str(key);
    if (!e->key) return -1;
    e->value = value;
    obj->as.obj.n += 1;
    return 0;
}

void pksc_value_free(pksc_value *v) {
    if (!v) return;
    switch (v->kind) {
        case PKSC_V_NULL:
        case PKSC_V_BOOL:
        case PKSC_V_INT:
            break;
        case PKSC_V_STR:
            free(v->as.s);
            break;
        case PKSC_V_ARR:
            for (size_t i = 0; i < v->as.arr.n; i++) {
                pksc_value_free(v->as.arr.items[i]);
            }
            free(v->as.arr.items);
            break;
        case PKSC_V_OBJ:
            for (size_t i = 0; i < v->as.obj.n; i++) {
                free(v->as.obj.entries[i].key);
                pksc_value_free(v->as.obj.entries[i].value);
            }
            free(v->as.obj.entries);
            break;
    }
    free(v);
}

/* --- JCS encoder -------------------------------------------------------- */

static int encode_value(pksc_bytes *out, const pksc_value *v);

static int encode_string(pksc_bytes *out, const char *s) {
    if (pksc_bytes_append_byte(out, '"') != 0) return -1;
    /* Iterate raw bytes. UTF-8 multi-byte continuation bytes always have
     * the high bit set, so they never collide with the ASCII control-char
     * range (< 0x20) we escape — emit them verbatim and the receiver
     * sees the same UTF-8 bytes the producer wrote. Matches the Rust
     * encoder's `c.chars()` shape because every char encodes to its UTF-8
     * bytes when pushed to a String. */
    for (const unsigned char *p = (const unsigned char *)s; *p; p++) {
        unsigned char c = *p;
        if (c == '"') {
            if (pksc_bytes_append(out, (const uint8_t *)"\\\"", 2) != 0) return -1;
        } else if (c == '\\') {
            if (pksc_bytes_append(out, (const uint8_t *)"\\\\", 2) != 0) return -1;
        } else if (c < 0x20) {
            char esc[7];
            int n = snprintf(esc, sizeof(esc), "\\u00%02x", c);
            if (n < 0) return -1;
            if (pksc_bytes_append(out, (const uint8_t *)esc, (size_t)n) != 0) return -1;
        } else {
            if (pksc_bytes_append_byte(out, c) != 0) return -1;
        }
    }
    return pksc_bytes_append_byte(out, '"');
}

/* Sort key indexes so we can iterate sorted without mutating the original. */
typedef struct {
    const pksc_kv *entries;
    size_t        *indexes;
    size_t         n;
} sort_ctx;

static int kv_compare(const void *pa, const void *pb, const pksc_kv *entries) {
    size_t ia = *(const size_t *)pa;
    size_t ib = *(const size_t *)pb;
    return strcmp(entries[ia].key, entries[ib].key);
}

/* qsort_r is platform-divergent; use a thread-local context. We don't
 * need thread safety inside a single encode call, so a static is fine
 * for serial usage. JCS encoding is leaf-only inside one call. */
static const pksc_kv *g_sort_entries;

static int kv_compare_static(const void *pa, const void *pb) {
    return kv_compare(pa, pb, g_sort_entries);
}

static int encode_object(pksc_bytes *out, const pksc_value *v) {
    if (pksc_bytes_append_byte(out, '{') != 0) return -1;
    size_t n = v->as.obj.n;
    if (n > 0) {
        size_t *idx = (size_t *)malloc(n * sizeof(*idx));
        if (!idx) return -1;
        for (size_t i = 0; i < n; i++) idx[i] = i;
        g_sort_entries = v->as.obj.entries;
        qsort(idx, n, sizeof(*idx), kv_compare_static);
        g_sort_entries = NULL;

        for (size_t i = 0; i < n; i++) {
            if (i > 0) {
                if (pksc_bytes_append_byte(out, ',') != 0) { free(idx); return -1; }
            }
            const pksc_kv *e = &v->as.obj.entries[idx[i]];
            if (encode_string(out, e->key) != 0) { free(idx); return -1; }
            if (pksc_bytes_append_byte(out, ':') != 0) { free(idx); return -1; }
            if (encode_value(out, e->value) != 0) { free(idx); return -1; }
        }
        free(idx);
    }
    return pksc_bytes_append_byte(out, '}');
}

static int encode_array(pksc_bytes *out, const pksc_value *v) {
    if (pksc_bytes_append_byte(out, '[') != 0) return -1;
    for (size_t i = 0; i < v->as.arr.n; i++) {
        if (i > 0) {
            if (pksc_bytes_append_byte(out, ',') != 0) return -1;
        }
        if (encode_value(out, v->as.arr.items[i]) != 0) return -1;
    }
    return pksc_bytes_append_byte(out, ']');
}

static int encode_value(pksc_bytes *out, const pksc_value *v) {
    if (!v) return -1;
    switch (v->kind) {
        case PKSC_V_NULL:
            return pksc_bytes_append(out, (const uint8_t *)"null", 4);
        case PKSC_V_BOOL:
            return v->as.b
                ? pksc_bytes_append(out, (const uint8_t *)"true", 4)
                : pksc_bytes_append(out, (const uint8_t *)"false", 5);
        case PKSC_V_INT: {
            char buf[32];
            int  n = snprintf(buf, sizeof(buf), "%lld", (long long)v->as.i);
            if (n < 0) return -1;
            return pksc_bytes_append(out, (const uint8_t *)buf, (size_t)n);
        }
        case PKSC_V_STR:
            return encode_string(out, v->as.s);
        case PKSC_V_ARR:
            return encode_array(out, v);
        case PKSC_V_OBJ:
            return encode_object(out, v);
    }
    return -1;
}

int pksc_jcs_encode(pksc_bytes *out, const pksc_value *v) {
    if (!out) return -1;
    return encode_value(out, v);
}

char *pksc_jcs_encode_string(const pksc_value *v) {
    pksc_bytes b;
    pksc_bytes_init(&b);
    if (pksc_jcs_encode(&b, v) != 0) {
        pksc_bytes_free(&b);
        return NULL;
    }
    /* NUL-terminate for caller convenience. */
    if (pksc_bytes_append_byte(&b, '\0') != 0) {
        pksc_bytes_free(&b);
        return NULL;
    }
    return (char *)b.data;
}
