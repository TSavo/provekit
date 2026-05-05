/* SPDX-License-Identifier: Apache-2.0 */
/*
 * provekit-self-contracts (C kit) — Side B native crypto library.
 *
 * Public C surface for:
 *   - JCS canonicalization (RFC 8785)
 *   - Deterministic CBOR encoding (RFC 8949 §4.2.1)
 *   - Ed25519 sign/verify (libsodium)
 *   - Proof envelope construction (.proof file format)
 *
 * Mirrors:
 *   implementations/rust/provekit-proof-envelope/src/{cbor,sign,proof}.rs
 *   implementations/rust/provekit-canonicalizer/src/{jcs,value}.rs
 *   implementations/cpp/provekit/proof-envelope/{cbor,sign_ed25519,proof_envelope}.{hpp,cpp}
 *   implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/{proof_envelope,signing,canonicalizer}.py
 *
 * Output is byte-identical to peer kits for the same canonical input.
 * The cross-kit golden fixture lives in tests/test_self_contracts.c
 * and matches the Rust reference output pinned in
 * implementations/python/provekit-lift-py-tests/tests/test_proof_envelope.py.
 *
 * Memory model: every output buffer is malloc'd by this library. Callers
 * own the bytes and must free() them. No global state.
 */

#ifndef PROVEKIT_SELF_CONTRACTS_H
#define PROVEKIT_SELF_CONTRACTS_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ----------------------------------------------------------------------- */
/* Byte buffer (growable; output container)                                */
/* ----------------------------------------------------------------------- */

typedef struct pksc_bytes {
    uint8_t *data;
    size_t len;
    size_t cap;
} pksc_bytes;

void pksc_bytes_init(pksc_bytes *b);
void pksc_bytes_free(pksc_bytes *b);
int  pksc_bytes_append(pksc_bytes *b, const uint8_t *src, size_t n);
int  pksc_bytes_append_byte(pksc_bytes *b, uint8_t c);

/* ----------------------------------------------------------------------- */
/* JSON Value tree (for JCS encoding)                                      */
/*                                                                         */
/* Minimal value model — null / bool / integer (i64) / string (UTF-8) /     */
/* array / object. Object insertion order is preserved; JCS sorts at emit. */
/* ----------------------------------------------------------------------- */

typedef enum {
    PKSC_V_NULL,
    PKSC_V_BOOL,
    PKSC_V_INT,
    PKSC_V_STR,
    PKSC_V_ARR,
    PKSC_V_OBJ,
} pksc_value_kind;

typedef struct pksc_value pksc_value;

typedef struct {
    char *key;            /* owned */
    pksc_value *value;    /* owned */
} pksc_kv;

struct pksc_value {
    pksc_value_kind kind;
    union {
        int            b;          /* PKSC_V_BOOL */
        int64_t        i;          /* PKSC_V_INT */
        char          *s;          /* PKSC_V_STR (owned) */
        struct {
            pksc_value **items;    /* owned array of owned ptrs */
            size_t       n;
        } arr;
        struct {
            pksc_kv     *entries;  /* owned array; key+value owned */
            size_t       n;
            size_t       cap;
        } obj;
    } as;
};

pksc_value *pksc_v_null(void);
pksc_value *pksc_v_bool(int b);
pksc_value *pksc_v_int(int64_t v);
pksc_value *pksc_v_str(const char *s);
pksc_value *pksc_v_arr_new(void);
pksc_value *pksc_v_obj_new(void);
int         pksc_v_arr_push(pksc_value *arr, pksc_value *item); /* takes ownership */
int         pksc_v_obj_set(pksc_value *obj, const char *key, pksc_value *value); /* takes ownership of value; key copied */
void        pksc_value_free(pksc_value *v);

/* JCS-encode `v` to UTF-8 bytes appended into `out`. Returns 0 on success. */
int pksc_jcs_encode(pksc_bytes *out, const pksc_value *v);

/* Convenience: JCS-encode and return a malloc'd NUL-terminated string. */
char *pksc_jcs_encode_string(const pksc_value *v);

/* ----------------------------------------------------------------------- */
/* Deterministic CBOR encoder (RFC 8949 §4.2.1)                            */
/*                                                                         */
/* Public helpers expose only the major types this protocol uses.          */
/* Map keys are sorted by callers via the pre-encoded-key form, see        */
/* pksc_proof_build below.                                                 */
/* ----------------------------------------------------------------------- */

typedef enum {
    PKSC_CBOR_UINT  = 0,
    PKSC_CBOR_BSTR  = 2,
    PKSC_CBOR_TSTR  = 3,
    PKSC_CBOR_ARRAY = 4,
    PKSC_CBOR_MAP   = 5,
} pksc_cbor_major;

int pksc_cbor_append_head(pksc_bytes *out, pksc_cbor_major major, uint64_t arg);
int pksc_cbor_encode_uint(pksc_bytes *out, uint64_t value);
int pksc_cbor_encode_bstr(pksc_bytes *out, const uint8_t *bytes, size_t n);
int pksc_cbor_encode_tstr(pksc_bytes *out, const char *utf8);
int pksc_cbor_encode_array_head(pksc_bytes *out, uint64_t count);
int pksc_cbor_encode_map_head(pksc_bytes *out, uint64_t count);

/* ----------------------------------------------------------------------- */
/* Ed25519 (libsodium-backed)                                              */
/*                                                                         */
/* Foundation v0 seed: 32 bytes of 0x42, identical across all kits. The    */
/* derived public key is shared via tools/foundation-keygen/.              */
/* ----------------------------------------------------------------------- */

#define PKSC_ED25519_SEED_LEN 32
#define PKSC_ED25519_SIG_LEN  64
#define PKSC_ED25519_PK_LEN   32

#define PKSC_ED25519_SIG_PREFIX "ed25519:"

extern const uint8_t PKSC_FOUNDATION_V0_SEED[PKSC_ED25519_SEED_LEN];

/*
 * Initialize libsodium. Idempotent. Must be called before any sign/verify.
 * Returns 0 on success, -1 on failure.
 */
int pksc_sodium_init(void);

/* Sign `message` with the Ed25519 key derived from `seed` (32 bytes).
 * Writes the 64-byte raw signature into `out_sig`. Returns 0 on success. */
int pksc_ed25519_sign_with_seed(uint8_t out_sig[PKSC_ED25519_SIG_LEN],
                                 const uint8_t seed[PKSC_ED25519_SEED_LEN],
                                 const uint8_t *message, size_t message_len);

/* Derive 32-byte raw public key from `seed`. Returns 0 on success. */
int pksc_ed25519_pubkey_from_seed(uint8_t out_pk[PKSC_ED25519_PK_LEN],
                                   const uint8_t seed[PKSC_ED25519_SEED_LEN]);

/* Verify raw signature `sig` over `message` against public key `pk`.
 * Returns 1 if valid, 0 if invalid. Never returns negative. */
int pksc_ed25519_verify(const uint8_t pk[PKSC_ED25519_PK_LEN],
                        const uint8_t sig[PKSC_ED25519_SIG_LEN],
                        const uint8_t *message, size_t message_len);

/* Spec-form helpers ("ed25519:" + base64(bytes)).
 *
 * Used for memento envelope per-claim `producerSignature` fields. The
 * proof-envelope catalog signature itself uses raw 64-byte CBOR bstr.
 */

/* Produce "ed25519:<base64-stdpad-of-sig>" (malloc'd, NUL-terminated). */
char *pksc_ed25519_sign_string(const uint8_t seed[PKSC_ED25519_SEED_LEN],
                                const uint8_t *message, size_t message_len);

/* Produce "ed25519:<base64-stdpad-of-pubkey>" (malloc'd, NUL-terminated). */
char *pksc_ed25519_pubkey_string(const uint8_t seed[PKSC_ED25519_SEED_LEN]);

/* Verify `sig_string` and `pubkey_string` (both spec form) against
 * `message`. Returns 1 if valid, 0 if invalid or malformed. */
int pksc_ed25519_verify_string(const char *pubkey_string,
                               const char *sig_string,
                               const uint8_t *message, size_t message_len);

/* ----------------------------------------------------------------------- */
/* BLAKE3-512 over a byte buffer                                           */
/*                                                                         */
/* Mirrors pk_hash_jcs from provekit-ir but takes raw bytes (the proof     */
/* envelope is CBOR, not JCS-JSON). Output is malloc'd                     */
/* "blake3-512:" + 128 lowercase hex chars, NUL-terminated.                */
/* ----------------------------------------------------------------------- */

char *pksc_blake3_512_cid(const uint8_t *data, size_t len);

/* ----------------------------------------------------------------------- */
/* Proof envelope builder (.proof file format)                             */
/* ----------------------------------------------------------------------- */

typedef struct {
    char    *key;     /* owned: full self-identifying CID, e.g. "blake3-512:..." */
    uint8_t *bytes;   /* owned: canonical bytes of this member */
    size_t   len;
} pksc_member;

typedef struct {
    char    *key;     /* owned */
    char    *value;   /* owned */
} pksc_meta_kv;

typedef struct pksc_proof_input {
    const char       *name;
    const char       *version;
    /* Optional: NULL or empty == omitted. */
    const char       *binary_cid;
    /* Optional: NULL or n_metadata == 0 == omitted. */
    pksc_meta_kv     *metadata;
    size_t            n_metadata;
    /* Required: members map (member CID => bytes). May be empty. */
    pksc_member      *members;
    size_t            n_members;
    /* Required: signer CID and ISO-8601 declaredAt timestamp. */
    const char       *signer_cid;
    const char       *declared_at;
    /* Required: 32-byte Ed25519 seed (use PKSC_FOUNDATION_V0_SEED for tests). */
    const uint8_t    *signer_seed;
} pksc_proof_input;

typedef struct pksc_proof_output {
    uint8_t *bytes;   /* owned malloc'd CBOR bytes */
    size_t   len;
    char    *cid;     /* owned malloc'd "blake3-512:<128 hex>" */
} pksc_proof_output;

/* Build a .proof envelope. On success returns 0 and populates `out`;
 * caller must free out->bytes and out->cid. Returns -1 on failure
 * (malloc / sodium / invalid input). */
int pksc_proof_build(const pksc_proof_input *in, pksc_proof_output *out);

/* Free fields populated by pksc_proof_build. Safe on zero-initialized. */
void pksc_proof_output_free(pksc_proof_output *out);

/* Verify a .proof envelope.
 *   - Recomputes BLAKE3-512(proof_bytes) and compares to expected_cid.
 *   - Decodes the CBOR catalog; checks required keys / sig length / shape.
 *   - Re-emits the unsigned body and ed25519-verifies the embedded
 *     signature against `signer_pk` (raw 32 bytes).
 * Returns 1 if all three checks pass, 0 otherwise. */
int pksc_proof_verify(const uint8_t *proof_bytes, size_t proof_len,
                      const char *expected_cid,
                       const uint8_t signer_pk[PKSC_ED25519_PK_LEN]);

/* ----------------------------------------------------------------------- */
/* Bridge v1.4 (layered envelope/header/body, tagged-union target).        */
/* Per protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.       */
/* ----------------------------------------------------------------------- */

/* Tagged-union target kind */
#define PKSC_BRIDGE_TARGET_CONTRACT    "contract"
#define PKSC_BRIDGE_TARGET_CONTRACTSET "contractSet"

/* Result of pksc_mint_bridge_v14. Caller must free →cid and →canonical_bytes. */
typedef struct pksc_bridge_v14_result {
    char   *cid;            /* malloc'd, NUL-terminated */
    uint8_t *canonical_bytes; /* malloc'd */
    size_t   canonical_len;
} pksc_bridge_v14_result;

void pksc_bridge_v14_result_free(pksc_bridge_v14_result *r);

/* Mint a v1.4 layered bridge memento.
 *   seed:       32-byte Ed25519 seed (e.g. PKSC_FOUNDATION_V0_SEED).
 *   declared_at: RFC 3339 timestamp for envelope.declaredAt.
 *   args:        header fields (name, source_symbol, source_layer,
 *                source_contract_cid) and target (kind, cid).
 *   metadata:    NULL-terminated array of {key, val} pairs; NULL entries omitted.
 * Returns 0 on success, -1 on error. */
int pksc_mint_bridge_v14(pksc_bridge_v14_result *out,
                         const uint8_t seed[PKSC_ED25519_SEED_LEN],
                         const char *declared_at,
                         const char *name,
                         const char *source_symbol,
                         const char *source_layer,
                         const char *source_contract_cid,
                         const char *target_kind,
                         const char *target_cid,
                         const char **metadata_keys,
                         const char **metadata_vals,
                         int metadata_count);

#ifdef __cplusplus
}
#endif

#endif  /* PROVEKIT_SELF_CONTRACTS_H */
