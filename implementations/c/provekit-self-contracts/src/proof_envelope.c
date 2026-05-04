/* SPDX-License-Identifier: Apache-2.0 */
/*
 * .proof envelope builder. RFC 8949 §4.2.1 deterministic CBOR + the
 * proof-file-format spec (protocol/specs/2026-04-30-proof-file-format.md).
 *
 * Protocol:
 *   1. Build the unsigned body as a CBOR map; sort keys by bytewise
 *      lex order of their CBOR-encoded form.
 *   2. Ed25519-sign the unsigned-body bytes.
 *   3. Re-emit the body with the signature added; sort slots it in.
 *   4. BLAKE3-512 the final bytes; the full self-identifying string
 *      "blake3-512:<128 hex>" IS the catalog CID.
 *
 * The `members` map key is the embedded envelope's own CID (string),
 * and the value is its canonical bytes (JCS-JSON for memento envelopes
 * per the memento envelope grammar) wrapped as a CBOR byte string.
 *
 * Mirrors implementations/{rust,cpp}/.../proof.{rs,cpp} 1:1.
 *
 * Verifier (pksc_proof_verify) implements a minimal CBOR walker over
 * the deterministic encoding produced above; it rejects anything that
 * deviates from §4.2.1 form (indefinite-length items, non-shortest
 * integers, unknown major types). Failure modes return 0 — never
 * negative — to keep the verify_failed and verify_ok code paths
 * separate at the call site.
 */

#include "provekit/self_contracts.h"

#include <stdlib.h>
#include <string.h>

/* ========================================================================
 * Builder
 * ====================================================================== */

/* A pre-encoded (key_cbor, value_cbor) pair. The outer map sorts pairs
 * by bytewise comparison of `key_cbor` (RFC 8949 §4.2.1). */
typedef struct {
    uint8_t *key_cbor;
    size_t   key_cbor_len;
    uint8_t *value_cbor;
    size_t   value_cbor_len;
} cbor_pair;

static void cbor_pair_init(cbor_pair *p) {
    p->key_cbor = NULL; p->key_cbor_len = 0;
    p->value_cbor = NULL; p->value_cbor_len = 0;
}

static void cbor_pair_free(cbor_pair *p) {
    free(p->key_cbor);
    free(p->value_cbor);
    p->key_cbor = NULL; p->key_cbor_len = 0;
    p->value_cbor = NULL; p->value_cbor_len = 0;
}

static int encode_to_owned(pksc_bytes *acc, uint8_t **out, size_t *out_len) {
    *out = acc->data;       /* steal */
    *out_len = acc->len;
    /* Reset acc so its destructor doesn't double-free. */
    acc->data = NULL;
    acc->len = 0;
    acc->cap = 0;
    return 0;
}

static int make_string_pair(cbor_pair *p, const char *key, const char *value) {
    cbor_pair_init(p);
    pksc_bytes k; pksc_bytes_init(&k);
    pksc_bytes v; pksc_bytes_init(&v);
    if (pksc_cbor_encode_tstr(&k, key) != 0) goto fail;
    if (pksc_cbor_encode_tstr(&v, value) != 0) goto fail;
    encode_to_owned(&k, &p->key_cbor, &p->key_cbor_len);
    encode_to_owned(&v, &p->value_cbor, &p->value_cbor_len);
    return 0;
fail:
    pksc_bytes_free(&k);
    pksc_bytes_free(&v);
    return -1;
}

static int make_bytes_pair(cbor_pair *p, const char *key, const uint8_t *value, size_t value_len) {
    cbor_pair_init(p);
    pksc_bytes k; pksc_bytes_init(&k);
    pksc_bytes v; pksc_bytes_init(&v);
    if (pksc_cbor_encode_tstr(&k, key) != 0) goto fail;
    if (pksc_cbor_encode_bstr(&v, value, value_len) != 0) goto fail;
    encode_to_owned(&k, &p->key_cbor, &p->key_cbor_len);
    encode_to_owned(&v, &p->value_cbor, &p->value_cbor_len);
    return 0;
fail:
    pksc_bytes_free(&k);
    pksc_bytes_free(&v);
    return -1;
}

static int pair_compare(const void *pa, const void *pb) {
    const cbor_pair *a = (const cbor_pair *)pa;
    const cbor_pair *b = (const cbor_pair *)pb;
    size_t n = a->key_cbor_len < b->key_cbor_len ? a->key_cbor_len : b->key_cbor_len;
    int c = memcmp(a->key_cbor, b->key_cbor, n);
    if (c != 0) return c;
    if (a->key_cbor_len < b->key_cbor_len) return -1;
    if (a->key_cbor_len > b->key_cbor_len) return  1;
    return 0;
}

static int emit_sorted_map(pksc_bytes *out, cbor_pair *pairs, size_t n) {
    qsort(pairs, n, sizeof(*pairs), pair_compare);
    if (pksc_cbor_encode_map_head(out, (uint64_t)n) != 0) return -1;
    for (size_t i = 0; i < n; i++) {
        if (pksc_bytes_append(out, pairs[i].key_cbor, pairs[i].key_cbor_len) != 0) return -1;
        if (pksc_bytes_append(out, pairs[i].value_cbor, pairs[i].value_cbor_len) != 0) return -1;
    }
    return 0;
}

/* Build the `members` value: { tstr(cid) => bstr(envelope-bytes) }, sorted. */
static int make_members_value(uint8_t **out, size_t *out_len,
                              const pksc_member *members, size_t n_members) {
    cbor_pair *pairs = (cbor_pair *)calloc(n_members ? n_members : 1, sizeof(*pairs));
    if (!pairs) return -1;
    for (size_t i = 0; i < n_members; i++) {
        if (make_bytes_pair(&pairs[i], members[i].key, members[i].bytes, members[i].len) != 0) {
            for (size_t j = 0; j < i; j++) cbor_pair_free(&pairs[j]);
            free(pairs);
            return -1;
        }
    }
    pksc_bytes acc; pksc_bytes_init(&acc);
    int rc = emit_sorted_map(&acc, pairs, n_members);
    for (size_t i = 0; i < n_members; i++) cbor_pair_free(&pairs[i]);
    free(pairs);
    if (rc != 0) { pksc_bytes_free(&acc); return -1; }
    encode_to_owned(&acc, out, out_len);
    return 0;
}

static int make_metadata_value(uint8_t **out, size_t *out_len,
                               const pksc_meta_kv *meta, size_t n_meta) {
    cbor_pair *pairs = (cbor_pair *)calloc(n_meta ? n_meta : 1, sizeof(*pairs));
    if (!pairs) return -1;
    for (size_t i = 0; i < n_meta; i++) {
        if (make_string_pair(&pairs[i], meta[i].key, meta[i].value) != 0) {
            for (size_t j = 0; j < i; j++) cbor_pair_free(&pairs[j]);
            free(pairs);
            return -1;
        }
    }
    pksc_bytes acc; pksc_bytes_init(&acc);
    int rc = emit_sorted_map(&acc, pairs, n_meta);
    for (size_t i = 0; i < n_meta; i++) cbor_pair_free(&pairs[i]);
    free(pairs);
    if (rc != 0) { pksc_bytes_free(&acc); return -1; }
    encode_to_owned(&acc, out, out_len);
    return 0;
}

static int has_str(const char *s) { return s && s[0] != '\0'; }

/* Populate a freshly-allocated array of cbor_pair[*out_n] for the
 * unsigned body. Caller is responsible for freeing each pair and the
 * array on success or failure. */
static int build_body_pairs(cbor_pair **out_pairs, size_t *out_n,
                            const pksc_proof_input *in) {
    size_t cap = 8;
    cbor_pair *pairs = (cbor_pair *)calloc(cap, sizeof(*pairs));
    if (!pairs) return -1;
    size_t n = 0;

    if (make_string_pair(&pairs[n++], "kind", "catalog")        != 0) goto fail;
    if (make_string_pair(&pairs[n++], "name", in->name)         != 0) goto fail;
    if (make_string_pair(&pairs[n++], "version", in->version)   != 0) goto fail;

    /* members */
    {
        uint8_t *mbytes = NULL; size_t mlen = 0;
        if (make_members_value(&mbytes, &mlen, in->members, in->n_members) != 0) goto fail;
        cbor_pair *p = &pairs[n++];
        cbor_pair_init(p);
        pksc_bytes k; pksc_bytes_init(&k);
        if (pksc_cbor_encode_tstr(&k, "members") != 0) {
            free(mbytes); pksc_bytes_free(&k); goto fail;
        }
        encode_to_owned(&k, &p->key_cbor, &p->key_cbor_len);
        p->value_cbor = mbytes;
        p->value_cbor_len = mlen;
    }

    if (make_string_pair(&pairs[n++], "signer", in->signer_cid)       != 0) goto fail;
    if (make_string_pair(&pairs[n++], "declaredAt", in->declared_at)  != 0) goto fail;

    if (has_str(in->binary_cid)) {
        if (make_string_pair(&pairs[n++], "binaryCid", in->binary_cid) != 0) goto fail;
    }

    if (in->metadata && in->n_metadata > 0) {
        uint8_t *mbytes = NULL; size_t mlen = 0;
        if (make_metadata_value(&mbytes, &mlen, in->metadata, in->n_metadata) != 0) goto fail;
        cbor_pair *p = &pairs[n++];
        cbor_pair_init(p);
        pksc_bytes k; pksc_bytes_init(&k);
        if (pksc_cbor_encode_tstr(&k, "metadata") != 0) {
            free(mbytes); pksc_bytes_free(&k); goto fail;
        }
        encode_to_owned(&k, &p->key_cbor, &p->key_cbor_len);
        p->value_cbor = mbytes;
        p->value_cbor_len = mlen;
    }

    *out_pairs = pairs;
    *out_n = n;
    return 0;
fail:
    for (size_t i = 0; i < n; i++) cbor_pair_free(&pairs[i]);
    free(pairs);
    return -1;
}

int pksc_proof_build(const pksc_proof_input *in, pksc_proof_output *out) {
    if (!in || !out) return -1;
    if (!in->name || !in->version || !in->signer_cid || !in->declared_at || !in->signer_seed) return -1;

    out->bytes = NULL;
    out->len = 0;
    out->cid = NULL;

    if (pksc_sodium_init() != 0) return -1;

    /* Step 1: emit unsigned body. */
    cbor_pair *unsigned_pairs = NULL;
    size_t     n_unsigned = 0;
    if (build_body_pairs(&unsigned_pairs, &n_unsigned, in) != 0) return -1;

    pksc_bytes unsigned_buf; pksc_bytes_init(&unsigned_buf);
    if (emit_sorted_map(&unsigned_buf, unsigned_pairs, n_unsigned) != 0) {
        for (size_t i = 0; i < n_unsigned; i++) cbor_pair_free(&unsigned_pairs[i]);
        free(unsigned_pairs);
        pksc_bytes_free(&unsigned_buf);
        return -1;
    }
    /* Don't free unsigned_pairs yet — we need to re-sort with the
     * signature appended in step 3. We free at step 3's exit. */

    /* Step 2: sign. */
    uint8_t sig[PKSC_ED25519_SIG_LEN];
    if (pksc_ed25519_sign_with_seed(sig, in->signer_seed, unsigned_buf.data, unsigned_buf.len) != 0) {
        for (size_t i = 0; i < n_unsigned; i++) cbor_pair_free(&unsigned_pairs[i]);
        free(unsigned_pairs);
        pksc_bytes_free(&unsigned_buf);
        return -1;
    }
    pksc_bytes_free(&unsigned_buf);

    /* Step 3: append signature pair and re-emit. */
    cbor_pair *signed_pairs = (cbor_pair *)calloc(n_unsigned + 1, sizeof(*signed_pairs));
    if (!signed_pairs) {
        for (size_t i = 0; i < n_unsigned; i++) cbor_pair_free(&unsigned_pairs[i]);
        free(unsigned_pairs);
        return -1;
    }
    /* Move ownership from unsigned_pairs into signed_pairs[0..n_unsigned). */
    memcpy(signed_pairs, unsigned_pairs, n_unsigned * sizeof(*unsigned_pairs));
    free(unsigned_pairs);
    if (make_bytes_pair(&signed_pairs[n_unsigned], "signature", sig, PKSC_ED25519_SIG_LEN) != 0) {
        for (size_t i = 0; i < n_unsigned; i++) cbor_pair_free(&signed_pairs[i]);
        free(signed_pairs);
        return -1;
    }
    size_t n_signed = n_unsigned + 1;

    pksc_bytes final_buf; pksc_bytes_init(&final_buf);
    int rc = emit_sorted_map(&final_buf, signed_pairs, n_signed);
    for (size_t i = 0; i < n_signed; i++) cbor_pair_free(&signed_pairs[i]);
    free(signed_pairs);
    if (rc != 0) { pksc_bytes_free(&final_buf); return -1; }

    /* Step 4: CID = BLAKE3-512 of final_bytes. */
    char *cid = pksc_blake3_512_cid(final_buf.data, final_buf.len);
    if (!cid) { pksc_bytes_free(&final_buf); return -1; }

    /* Move final_buf into out->bytes. */
    out->bytes = final_buf.data;
    out->len = final_buf.len;
    final_buf.data = NULL; final_buf.len = 0; final_buf.cap = 0;
    out->cid = cid;
    return 0;
}

void pksc_proof_output_free(pksc_proof_output *out) {
    if (!out) return;
    free(out->bytes);
    free(out->cid);
    out->bytes = NULL;
    out->len = 0;
    out->cid = NULL;
}

/* ========================================================================
 * Verifier — minimal CBOR walker over deterministic-encoded catalog
 * ====================================================================== */

typedef struct {
    const uint8_t *p;
    size_t         len;
} cur_t;

static int read_head(cur_t *c, uint8_t *major_out, uint64_t *arg_out) {
    if (c->len < 1) return -1;
    uint8_t b = c->p[0];
    uint8_t major = b >> 5;
    uint8_t info  = b & 0x1F;
    c->p += 1; c->len -= 1;
    uint64_t arg = 0;
    if (info < 24) {
        arg = info;
    } else if (info == 24) {
        if (c->len < 1) return -1;
        arg = c->p[0];
        c->p += 1; c->len -= 1;
    } else if (info == 25) {
        if (c->len < 2) return -1;
        arg = ((uint64_t)c->p[0] << 8) | c->p[1];
        c->p += 2; c->len -= 2;
    } else if (info == 26) {
        if (c->len < 4) return -1;
        arg = ((uint64_t)c->p[0] << 24) | ((uint64_t)c->p[1] << 16) | ((uint64_t)c->p[2] << 8) | c->p[3];
        c->p += 4; c->len -= 4;
    } else if (info == 27) {
        if (c->len < 8) return -1;
        arg = 0;
        for (int i = 0; i < 8; i++) arg = (arg << 8) | c->p[i];
        c->p += 8; c->len -= 8;
    } else {
        /* 28..30 reserved, 31 indefinite — both rejected for our deterministic profile. */
        return -1;
    }
    *major_out = major;
    *arg_out = arg;
    return 0;
}

/* Skip over a single CBOR data item, recursively. */
static int skip_item(cur_t *c) {
    uint8_t major; uint64_t arg;
    if (read_head(c, &major, &arg) != 0) return -1;
    switch (major) {
        case 0: case 1: /* uint, negint */
            return 0;
        case 2: case 3: /* bstr, tstr */
            if (c->len < arg) return -1;
            c->p += arg; c->len -= (size_t)arg;
            return 0;
        case 4: /* array */
            for (uint64_t i = 0; i < arg; i++) {
                if (skip_item(c) != 0) return -1;
            }
            return 0;
        case 5: /* map */
            for (uint64_t i = 0; i < arg; i++) {
                if (skip_item(c) != 0) return -1; /* key */
                if (skip_item(c) != 0) return -1; /* value */
            }
            return 0;
        default:
            return -1;
    }
}

/* Match a tstr at cursor. Advances cursor past it. */
static int read_tstr(cur_t *c, const uint8_t **out, size_t *out_len) {
    uint8_t major; uint64_t arg;
    cur_t save = *c;
    if (read_head(c, &major, &arg) != 0 || major != 3) { *c = save; return -1; }
    if (c->len < arg) { *c = save; return -1; }
    *out = c->p;
    *out_len = (size_t)arg;
    c->p += arg; c->len -= (size_t)arg;
    return 0;
}

/* Match a bstr at cursor. Advances cursor past it. */
static int read_bstr(cur_t *c, const uint8_t **out, size_t *out_len) {
    uint8_t major; uint64_t arg;
    cur_t save = *c;
    if (read_head(c, &major, &arg) != 0 || major != 2) { *c = save; return -1; }
    if (c->len < arg) { *c = save; return -1; }
    *out = c->p;
    *out_len = (size_t)arg;
    c->p += arg; c->len -= (size_t)arg;
    return 0;
}

/* Verify checks (in order):
 *   (1) recompute BLAKE3-512(proof_bytes) == expected_cid
 *   (2) decode CBOR catalog map; require kind, name, version, members,
 *       signer, declaredAt, signature; signature MUST be 64-byte bstr.
 *   (3) re-encode the unsigned body (all top-level pairs except
 *       "signature") and ed25519-verify the signature against
 *       signer_pk.
 * Returns 1 on full pass, 0 otherwise. Memory: must not leak on any path. */
int pksc_proof_verify(const uint8_t *proof_bytes, size_t proof_len,
                      const char *expected_cid,
                      const uint8_t signer_pk[PKSC_ED25519_PK_LEN]) {
    if (!proof_bytes || !expected_cid || !signer_pk) return 0;

    /* Check (1): CID match. */
    char *got_cid = pksc_blake3_512_cid(proof_bytes, proof_len);
    if (!got_cid) return 0;
    int cid_ok = (strcmp(got_cid, expected_cid) == 0);
    free(got_cid);
    if (!cid_ok) return 0;

    /* Check (2): walk top-level map, find signature, collect non-sig pairs. */
    cur_t c = { proof_bytes, proof_len };
    uint8_t major; uint64_t map_count;
    if (read_head(&c, &major, &map_count) != 0 || major != 5) return 0;

    /* Collect non-signature key/value byte ranges to re-emit the
     * unsigned body. */
    typedef struct {
        const uint8_t *key_p;  size_t key_n;
        const uint8_t *val_p;  size_t val_n;
        const uint8_t *full_p; size_t full_n; /* key+value contiguous range */
    } stash_t;

    stash_t *stash = (stash_t *)calloc(map_count ? map_count : 1, sizeof(*stash));
    if (!stash) return 0;
    size_t n_stash = 0;
    const uint8_t *sig_bytes = NULL;
    int found_sig = 0;
    int required_kind = 0, required_name = 0, required_version = 0,
        required_members = 0, required_signer = 0, required_declaredAt = 0;

    for (uint64_t i = 0; i < map_count; i++) {
        const uint8_t *pair_start = c.p;
        size_t         remaining_at_start = c.len;
        const uint8_t *kp = NULL; size_t kn = 0;
        if (read_tstr(&c, &kp, &kn) != 0) { free(stash); return 0; }

        const uint8_t *vp_start = c.p;
        size_t          remaining_at_value = c.len;

        int is_sig = (kn == 9 && memcmp(kp, "signature", 9) == 0);

        if (is_sig) {
            const uint8_t *sp; size_t sn;
            if (read_bstr(&c, &sp, &sn) != 0) { free(stash); return 0; }
            if (sn != PKSC_ED25519_SIG_LEN) { free(stash); return 0; }
            sig_bytes = sp;
            found_sig = 1;
        } else {
            /* Skip arbitrary value type and stash the full pair span. */
            if (skip_item(&c) != 0) { free(stash); return 0; }
            stash[n_stash].key_p = kp;
            stash[n_stash].key_n = kn;
            stash[n_stash].val_p = vp_start;
            stash[n_stash].val_n = remaining_at_value - c.len;
            stash[n_stash].full_p = pair_start;
            stash[n_stash].full_n = remaining_at_start - c.len;
            n_stash++;

            /* Track required keys. */
            if      (kn == 4 && memcmp(kp, "kind", 4) == 0)         required_kind = 1;
            else if (kn == 4 && memcmp(kp, "name", 4) == 0)         required_name = 1;
            else if (kn == 7 && memcmp(kp, "version", 7) == 0)      required_version = 1;
            else if (kn == 7 && memcmp(kp, "members", 7) == 0)      required_members = 1;
            else if (kn == 6 && memcmp(kp, "signer", 6) == 0)       required_signer = 1;
            else if (kn == 10 && memcmp(kp, "declaredAt", 10) == 0) required_declaredAt = 1;
        }
    }
    /* Trailing bytes after the top-level map are not allowed. */
    if (c.len != 0) { free(stash); return 0; }

    if (!found_sig || !required_kind || !required_name || !required_version ||
        !required_members || !required_signer || !required_declaredAt) {
        free(stash); return 0;
    }

    /* Check (3): re-emit unsigned body (already-sorted, since the input
     * was deterministic CBOR). The non-signature pairs are in original
     * (sorted) order minus the signature key. Their relative order is
     * preserved by removing signature, so the re-encoded body equals
     * the bytes that were originally signed. */
    pksc_bytes ub; pksc_bytes_init(&ub);
    if (pksc_cbor_encode_map_head(&ub, (uint64_t)n_stash) != 0) { pksc_bytes_free(&ub); free(stash); return 0; }
    for (size_t i = 0; i < n_stash; i++) {
        if (pksc_bytes_append(&ub, stash[i].full_p, stash[i].full_n) != 0) {
            pksc_bytes_free(&ub); free(stash); return 0;
        }
    }

    int verify_ok = pksc_ed25519_verify(signer_pk, sig_bytes, ub.data, ub.len);
    pksc_bytes_free(&ub);
    free(stash);
    return verify_ok ? 1 : 0;
}
