/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Ed25519 (libsodium-backed). v1.1.0 of the protocol mandates
 * self-identifying signatures of the form:
 *
 *   "ed25519:" + base64-stdpad(64-byte-signature)
 *
 * The .proof catalog signature itself is stored as a RAW 64-byte CBOR
 * byte string (NOT the prefixed string form). The prefixed-string form
 * is only used for memento envelope `producerSignature` fields.
 *
 * Foundation v0 seed: 32 bytes of 0x42 (publicly-known test seed,
 * documented in tools/foundation-keygen/src/lib.rs). v1 of the
 * substrate uses HSM-generated keys.
 */

#include "provekit/self_contracts.h"

#include <sodium.h>
#include <stdlib.h>
#include <string.h>

const uint8_t PKSC_FOUNDATION_V0_SEED[PKSC_ED25519_SEED_LEN] = {
    0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42,
    0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42,
    0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42,
    0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42,
};

int pksc_sodium_init(void) {
    /* sodium_init() returns 0 on first success, 1 if already initialized,
     * -1 on failure. Treat anything != -1 as success. */
    int rc = sodium_init();
    return (rc == -1) ? -1 : 0;
}

int pksc_ed25519_sign_with_seed(uint8_t out_sig[PKSC_ED25519_SIG_LEN],
                                 const uint8_t seed[PKSC_ED25519_SEED_LEN],
                                 const uint8_t *message, size_t message_len) {
    if (!out_sig || !seed) return -1;
    if (message_len > 0 && !message) return -1;
    if (pksc_sodium_init() != 0) return -1;

    /* libsodium derives a 64-byte secret key (seed || pubkey) from a
     * 32-byte Ed25519 seed via crypto_sign_seed_keypair. crypto_sign_detached
     * then produces the 64-byte raw signature. Byte-equivalent to
     * ed25519-dalek's SigningKey::from_bytes(seed).sign(msg). */
    unsigned char pk[crypto_sign_PUBLICKEYBYTES];
    unsigned char sk[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pk, sk, seed) != 0) return -1;

    unsigned long long siglen = 0;
    int rc = crypto_sign_detached(out_sig, &siglen, message, (unsigned long long)message_len, sk);
    sodium_memzero(sk, sizeof sk);
    if (rc != 0 || siglen != PKSC_ED25519_SIG_LEN) return -1;
    return 0;
}

int pksc_ed25519_pubkey_from_seed(uint8_t out_pk[PKSC_ED25519_PK_LEN],
                                   const uint8_t seed[PKSC_ED25519_SEED_LEN]) {
    if (!out_pk || !seed) return -1;
    if (pksc_sodium_init() != 0) return -1;

    unsigned char sk[crypto_sign_SECRETKEYBYTES];
    int rc = crypto_sign_seed_keypair(out_pk, sk, seed);
    sodium_memzero(sk, sizeof sk);
    return rc == 0 ? 0 : -1;
}

int pksc_ed25519_verify(const uint8_t pk[PKSC_ED25519_PK_LEN],
                        const uint8_t sig[PKSC_ED25519_SIG_LEN],
                        const uint8_t *message, size_t message_len) {
    if (!pk || !sig) return 0;
    if (message_len > 0 && !message) return 0;
    if (pksc_sodium_init() != 0) return 0;
    int rc = crypto_sign_verify_detached(sig, message, (unsigned long long)message_len, pk);
    return rc == 0 ? 1 : 0;
}

/* --- base64-stdpad helpers (RFC 4648 §4 with `=` padding) --------------- */

static const char B64_ALPHA[64] = {
    'A','B','C','D','E','F','G','H','I','J','K','L','M','N','O','P',
    'Q','R','S','T','U','V','W','X','Y','Z','a','b','c','d','e','f',
    'g','h','i','j','k','l','m','n','o','p','q','r','s','t','u','v',
    'w','x','y','z','0','1','2','3','4','5','6','7','8','9','+','/'
};

static char *b64_encode_stdpad(const uint8_t *data, size_t n) {
    size_t out_len = ((n + 2) / 3) * 4;
    char *out = (char *)malloc(out_len + 1);
    if (!out) return NULL;
    size_t oi = 0, di = 0;
    while (di + 3 <= n) {
        uint32_t v = ((uint32_t)data[di] << 16) | ((uint32_t)data[di+1] << 8) | (uint32_t)data[di+2];
        out[oi++] = B64_ALPHA[(v >> 18) & 0x3F];
        out[oi++] = B64_ALPHA[(v >> 12) & 0x3F];
        out[oi++] = B64_ALPHA[(v >>  6) & 0x3F];
        out[oi++] = B64_ALPHA[ v        & 0x3F];
        di += 3;
    }
    size_t rem = n - di;
    if (rem == 1) {
        uint32_t v = (uint32_t)data[di] << 16;
        out[oi++] = B64_ALPHA[(v >> 18) & 0x3F];
        out[oi++] = B64_ALPHA[(v >> 12) & 0x3F];
        out[oi++] = '=';
        out[oi++] = '=';
    } else if (rem == 2) {
        uint32_t v = ((uint32_t)data[di] << 16) | ((uint32_t)data[di+1] << 8);
        out[oi++] = B64_ALPHA[(v >> 18) & 0x3F];
        out[oi++] = B64_ALPHA[(v >> 12) & 0x3F];
        out[oi++] = B64_ALPHA[(v >>  6) & 0x3F];
        out[oi++] = '=';
    }
    out[oi] = '\0';
    return out;
}

static int b64_value(unsigned char c) {
    if (c >= 'A' && c <= 'Z') return c - 'A';
    if (c >= 'a' && c <= 'z') return c - 'a' + 26;
    if (c >= '0' && c <= '9') return c - '0' + 52;
    if (c == '+') return 62;
    if (c == '/') return 63;
    return -1;
}

/* Decode strict-padded standard base64. Returns 0 on success and
 * writes the decoded length to *out_len; -1 on any malformed input. */
static int b64_decode_stdpad(const char *s, uint8_t *out, size_t out_cap, size_t *out_len) {
    if (!s || !out || !out_len) return -1;
    size_t n = strlen(s);
    if (n % 4 != 0) return -1;
    size_t produced = 0;
    for (size_t i = 0; i < n; i += 4) {
        int a = b64_value((unsigned char)s[i]);
        int b = b64_value((unsigned char)s[i+1]);
        int c = (s[i+2] == '=') ? -2 : b64_value((unsigned char)s[i+2]);
        int d = (s[i+3] == '=') ? -2 : b64_value((unsigned char)s[i+3]);
        if (a < 0 || b < 0) return -1;
        /* Padding only allowed at the end and only `==` or `=`. */
        int pad_count = (c == -2) + (d == -2);
        if (pad_count != 0 && i + 4 != n) return -1;
        if (c == -2 && d != -2) return -1;
        if (produced + (3 - pad_count) > out_cap) return -1;
        out[produced++] = (uint8_t)((a << 2) | (b >> 4));
        if (c >= 0) out[produced++] = (uint8_t)(((b & 0x0F) << 4) | (c >> 2));
        if (d >= 0) out[produced++] = (uint8_t)(((c & 0x03) << 6) | d);
    }
    *out_len = produced;
    return 0;
}

char *pksc_ed25519_sign_string(const uint8_t seed[PKSC_ED25519_SEED_LEN],
                                const uint8_t *message, size_t message_len) {
    uint8_t sig[PKSC_ED25519_SIG_LEN];
    if (pksc_ed25519_sign_with_seed(sig, seed, message, message_len) != 0) return NULL;
    char *b64 = b64_encode_stdpad(sig, PKSC_ED25519_SIG_LEN);
    if (!b64) return NULL;
    size_t prefix_len = strlen(PKSC_ED25519_SIG_PREFIX);
    size_t b64_len = strlen(b64);
    char *out = (char *)malloc(prefix_len + b64_len + 1);
    if (!out) { free(b64); return NULL; }
    memcpy(out, PKSC_ED25519_SIG_PREFIX, prefix_len);
    memcpy(out + prefix_len, b64, b64_len + 1);
    free(b64);
    return out;
}

char *pksc_ed25519_pubkey_string(const uint8_t seed[PKSC_ED25519_SEED_LEN]) {
    uint8_t pk[PKSC_ED25519_PK_LEN];
    if (pksc_ed25519_pubkey_from_seed(pk, seed) != 0) return NULL;
    char *b64 = b64_encode_stdpad(pk, PKSC_ED25519_PK_LEN);
    if (!b64) return NULL;
    size_t prefix_len = strlen(PKSC_ED25519_SIG_PREFIX);
    size_t b64_len = strlen(b64);
    char *out = (char *)malloc(prefix_len + b64_len + 1);
    if (!out) { free(b64); return NULL; }
    memcpy(out, PKSC_ED25519_SIG_PREFIX, prefix_len);
    memcpy(out + prefix_len, b64, b64_len + 1);
    free(b64);
    return out;
}

int pksc_ed25519_verify_string(const char *pubkey_string,
                               const char *sig_string,
                               const uint8_t *message, size_t message_len) {
    if (!pubkey_string || !sig_string) return 0;
    size_t prefix_len = strlen(PKSC_ED25519_SIG_PREFIX);
    if (strncmp(pubkey_string, PKSC_ED25519_SIG_PREFIX, prefix_len) != 0) return 0;
    if (strncmp(sig_string,    PKSC_ED25519_SIG_PREFIX, prefix_len) != 0) return 0;

    uint8_t pk[PKSC_ED25519_PK_LEN];
    uint8_t sig[PKSC_ED25519_SIG_LEN];
    size_t  pk_n = 0, sig_n = 0;
    if (b64_decode_stdpad(pubkey_string + prefix_len, pk, sizeof pk, &pk_n) != 0) return 0;
    if (b64_decode_stdpad(sig_string    + prefix_len, sig, sizeof sig, &sig_n) != 0) return 0;
    if (pk_n != PKSC_ED25519_PK_LEN || sig_n != PKSC_ED25519_SIG_LEN) return 0;
    return pksc_ed25519_verify(pk, sig, message, message_len);
}
